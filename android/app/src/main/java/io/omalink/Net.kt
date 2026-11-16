package io.omalink

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import org.json.JSONObject

/**
 * The network link to the computer, beside Bluetooth: the same messages,
 * plus files and the screen, over any network once paired with icons.
 *
 * The Rust core does the networking and the icon check; this reads its
 * events on a thread of its own and hands them to whoever shows them.
 */
object Net {
    private const val TAG = "Omalink"
    const val VERIFY_NOTIFICATION = 3

    /** A computer that can be paired with right now. */
    data class Computer(val id: String, val name: String, val via: String)

    private val main = Handler(Looper.getMainLooper())
    @Volatile private var started = false
    @Volatile var id = ""
        private set
    @Volatile var connected = false
        private set
    @Volatile var peer = ""
        private set
    @Volatile var route = ""
        private set

    val computers = LinkedHashMap<String, Computer>()

    /** Three icons to choose from, while a verification waits for a tap. */
    @Volatile var verify: JSONObject? = null
        private set
    /** How the last verification ended, until someone has shown it. */
    @Volatile var finished: JSONObject? = null

    /** Called on the main thread when anything above changes. */
    var onChange: (() -> Unit)? = null
    /** A mirroring session was verified: Android's own consent comes next. */
    var onMirrorApproved: ((maxSize: Int) -> Unit)? = null

    fun start(service: LinkService) {
        if (started) return
        started = true
        Thread({
            val dir = service.filesDir.resolve("net").absolutePath
            val address = try { Core.netStart(service.applicationContext, dir, Build.MODEL ?: "Phone") } catch (e: Throwable) { null }
            if (address == null) {
                Log.w(TAG, "networking didn't start")
                started = false
                return@Thread
            }
            id = address
            watchNetwork(service)
            while (true) {
                val event = Core.netNext(1000) ?: continue
                try {
                    handle(service, JSONObject(event))
                } catch (e: Exception) {
                    Log.w(TAG, "network event failed: $e")
                }
            }
        }, "omalink-net-events").start()
    }

    fun paired(): List<Pair<String, String>> = try {
        val peers = org.json.JSONArray(if (started) Core.netPeers() else "[]")
        (0 until peers.length()).map { peers.getJSONObject(it).let { p -> p.getString("id") to p.optString("name") } }
    } catch (e: Exception) {
        emptyList()
    }

    fun send(json: String): Boolean = started && Core.netSend(json)

    fun pair(computer: Computer) {
        finished = null
        Core.netPair(computer.id, computer.name)
    }

    fun choose(index: Int) {
        verify = null
        Core.netChoose(index)
        changed()
    }

    fun decline() {
        verify = null
        Core.netDecline()
        changed()
    }

    fun forget(id: String) {
        Core.netForget(id)
        changed()
    }

    /** Over Bluetooth, the computer said where it can be reached. */
    fun invited(id: String, name: String) {
        if (started) Core.netInvited(id, name)
    }

    private fun changed() = main.post { onChange?.invoke() }

    private fun handle(service: LinkService, event: JSONObject) {
        when (event.getString("ev")) {
            "found" -> {
                val computer = Computer(event.getString("id"), event.optString("name").ifEmpty { "Computer" }, event.optString("via"))
                synchronized(computers) { computers[computer.id] = computer }
                changed()
            }
            "gone" -> {
                synchronized(computers) {
                    // One Bluetooth told us about stays: it isn't on this network anyway.
                    if (computers[event.getString("id")]?.via == "network") computers.remove(event.getString("id"))
                }
                changed()
            }
            "link" -> {
                connected = event.optBoolean("connected")
                if (connected) {
                    peer = event.optString("name")
                    route = event.optString("route")
                }
                service.linkChanged()
            }
            "message" -> service.received(JSONObject(event.getString("json")), fromNetwork = true)
            "verify" -> {
                verify = event
                finished = null
                changed()
                ask(service, event)
            }
            "verify_done" -> {
                verify = null
                finished = event
                service.getSystemService(NotificationManager::class.java).cancel(VERIFY_NOTIFICATION)
                changed()
                if (event.optString("purpose") == "mirror" && event.optBoolean("ok")) {
                    val size = event.optInt("max_size", 1600)
                    main.post { onMirrorApproved?.invoke(size) ?: MirrorActivity.start(service, size) }
                }
            }
            "file" -> service.transfers.receiveNet(
                event.getLong("handle"), event.getLong("id"), event.optString("name", "file"), event.optLong("size", -1),
            )
        }
    }

    /**
     * Puts the three icons in front of the person: straight away when the
     * app is on screen, otherwise through a notification, since Android
     * doesn't let a background app open a screen by itself.
     */
    private fun ask(service: LinkService, event: JSONObject) {
        val intent = Intent(service, PairActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(PairActivity.EXTRA_VERIFY, true)
        val mirror = event.optString("purpose") == "mirror"
        val name = event.optString("name").ifEmpty { "Your computer" }
        val notification = Notification.Builder(service, LinkService.ASK_CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle(if (mirror) "$name wants to see your screen" else "Finish pairing with $name")
            .setContentText("Tap, then choose the icon your computer shows")
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(service, 4, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
            .build()
        service.getSystemService(NotificationManager::class.java).notify(VERIFY_NOTIFICATION, notification)
        if (PairActivity.visible) main.post { PairActivity.current?.render() }
        else try { service.startActivity(intent) } catch (e: Exception) { }
    }

    /** A new network: look for the computer again rather than wait. */
    private fun watchNetwork(context: Context) {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return
        try {
            manager.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) = Core.netNetworkChanged()
                override fun onLost(network: Network) = Core.netNetworkChanged()
            })
        } catch (e: Exception) { }
    }
}
