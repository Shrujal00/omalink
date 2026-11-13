package io.omalink

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Base64
import android.widget.Toast
import org.json.JSONArray
import org.json.JSONObject
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.InputStream
import java.net.Inet4Address
import java.net.NetworkInterface
import java.net.ServerSocket
import java.net.Socket
import java.nio.ByteBuffer
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * Files to and from the computer, over the network.
 *
 * Bluetooth arranges each transfer and a TCP connection carries it. The
 * phone always listens and the computer connects, since computers tend to
 * firewall what comes in. Each file gets its own port and its own key, sent
 * over the Bluetooth link, and travels as AES-GCM records; the format is
 * written down in the protocol crate, under `transfer`.
 */
class Transfers(private val service: LinkService) {
    private companion object {
        const val CHUNK = 64 * 1024
        const val TAG = 16
        const val CHANNEL = "files"
        /** How long the computer has to connect once it is told where. */
        const val WAIT_MS = 20_000
    }

    private val main = Handler(Looper.getMainLooper())
    private val random = SecureRandom()
    private var notified = 100

    private fun say(text: String) = main.post { Toast.makeText(service, text, Toast.LENGTH_LONG).show() }

    /** This phone's addresses, Wi-Fi first; mobile data ones can't be reached from a computer. */
    private fun addresses(): JSONArray {
        val found = ArrayList<Pair<String, String>>()
        try {
            for (network in NetworkInterface.getNetworkInterfaces()) {
                if (!network.isUp || network.isLoopback) continue
                val name = network.name ?: ""
                if (name.startsWith("rmnet") || name.startsWith("ccmni") || name.startsWith("dummy")) continue
                for (address in network.inetAddresses) {
                    if (address is Inet4Address) found.add(name to (address.hostAddress ?: continue))
                }
            }
        } catch (e: Exception) { }
        found.sortBy { if (it.first.startsWith("wlan")) 0 else 1 }
        return JSONArray(found.map { it.second })
    }

    private class Listening(val server: ServerSocket, val key: ByteArray, val route: JSONObject)

    /** Opens a port for one file; null, having said why, when there is no network to use. */
    private fun listen(): Listening? {
        val addrs = addresses()
        if (addrs.length() == 0) {
            say("Connect the phone to the same Wi-Fi as your computer to send files")
            return null
        }
        val key = ByteArray(32).also(random::nextBytes)
        val server = ServerSocket(0).apply { soTimeout = WAIT_MS }
        val route = JSONObject()
            .put("port", server.localPort)
            .put("addrs", addrs)
            .put("key", Base64.encodeToString(key, Base64.NO_WRAP))
        return Listening(server, key, route)
    }

    private fun cipher(mode: Int, key: ByteArray, index: Long): Cipher {
        val nonce = ByteBuffer.allocate(12).putInt(0).putLong(index).array()
        return Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(mode, SecretKeySpec(key, "AES"), GCMParameterSpec(TAG * 8, nonce))
        }
    }

    /** Phone to computer: `input` is already open, so it outlives whoever shared it. */
    fun offer(name: String, size: Long, input: InputStream) {
        Thread {
            var socket: Socket? = null
            val listening = try { listen() } catch (e: Exception) { null }
            try {
                if (listening == null) return@Thread
                val id = System.currentTimeMillis() * 1000 + random.nextInt(1000)
                service.send(message("file_offer", listening.route.put("id", id).put("name", name).put("size", size)))
                socket = listening.server.accept()
                val out = DataOutputStream(socket.getOutputStream().buffered(CHUNK * 2))
                val chunk = ByteArray(CHUNK)
                var index = 0L
                while (true) {
                    // Fill the chunk; a short read is not the end of the file.
                    var filled = 0
                    while (filled < CHUNK) {
                        val count = input.read(chunk, filled, CHUNK - filled)
                        if (count < 0) break
                        filled += count
                    }
                    val record = cipher(Cipher.ENCRYPT_MODE, listening.key, index++).doFinal(chunk, 0, filled)
                    out.writeInt(record.size)
                    out.write(record)
                    if (filled == 0) break
                }
                out.flush()
                // Wait for the computer to hang up, so nothing is cut off.
                socket.soTimeout = 10_000
                try { socket.getInputStream().read() } catch (e: Exception) { }
                say("Sent $name to your computer")
            } catch (e: java.net.SocketTimeoutException) {
                say("Your computer couldn't reach the phone. Put both on the same Wi-Fi")
            } catch (e: Exception) {
                say("Couldn't send $name")
            } finally {
                try { input.close() } catch (e: Exception) { }
                try { socket?.close() } catch (e: Exception) { }
                try { listening?.server?.close() } catch (e: Exception) { }
            }
        }.start()
    }

    /** Computer to phone: the file lands in Downloads/Omalink. */
    fun receive(id: Long, name: String, size: Long) {
        Thread {
            fun saved(ok: Boolean, error: String = "") =
                service.send(message("file_saved", JSONObject().put("id", id).put("ok", ok).put("error", error)))
            val listening = try { listen() } catch (e: Exception) { null }
                ?: return@Thread saved(false, "The phone isn't on a network the computer can reach.")
            val resolver = service.contentResolver
            var uri: Uri? = null
            var socket: Socket? = null
            try {
                service.send(message("file_ready", listening.route.put("id", id)))
                socket = listening.server.accept()
                socket.soTimeout = 30_000
                // Only the last part of the name: it comes from another device.
                val safe = name.substringAfterLast('/').substringAfterLast('\\').ifEmpty { "file" }
                uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, safe)
                    put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Omalink")
                    put(MediaStore.Downloads.IS_PENDING, 1)
                }) ?: throw IllegalStateException("no place to save")
                val input = DataInputStream(socket.getInputStream().buffered(CHUNK * 2))
                val record = ByteArray(CHUNK + TAG)
                resolver.openOutputStream(uri)!!.use { out ->
                    var index = 0L
                    while (true) {
                        val length = input.readInt()
                        if (length < TAG || length > CHUNK + TAG) throw IllegalStateException("bad record")
                        input.readFully(record, 0, length)
                        val plain = cipher(Cipher.DECRYPT_MODE, listening.key, index++).doFinal(record, 0, length)
                        if (plain.isEmpty()) break
                        out.write(plain)
                    }
                }
                resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
                saved(true)
                arrived(safe, uri)
            } catch (e: Exception) {
                uri?.let { try { resolver.delete(it, null, null) } catch (e: Exception) { } }
                saved(false, if (e is java.net.SocketTimeoutException) "The computer couldn't reach the phone over the network." else "The file didn't arrive intact.")
            } finally {
                try { socket?.close() } catch (e: Exception) { }
                try { listening.server.close() } catch (e: Exception) { }
            }
        }.start()
    }

    /** A notification that opens the file. */
    private fun arrived(name: String, uri: Uri) {
        val manager = service.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Files from your computer", NotificationManager.IMPORTANCE_DEFAULT))
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, service.contentResolver.getType(uri) ?: "*/*")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        val id = synchronized(this) { notified++ }
        manager.notify(id, Notification.Builder(service, CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle("Received from your computer")
            .setContentText("$name is in Downloads/Omalink")
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(service, id, view, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
            .build())
    }
}
