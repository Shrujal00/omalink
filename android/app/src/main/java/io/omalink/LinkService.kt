package io.omalink

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothServerSocket
import android.bluetooth.BluetoothSocket
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.database.ContentObserver
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.Ringtone
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.provider.CallLog
import android.provider.Telephony
import android.telecom.TelecomManager
import android.telephony.SmsManager
import android.telephony.TelephonyManager
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import java.util.concurrent.LinkedBlockingQueue

/**
 * Keeps a Bluetooth channel open for the computer and answers what it asks.
 *
 * The phone listens; the computer connects. A foreground service is what
 * Android requires for that to keep working with the screen off.
 */
class LinkService : Service() {
    companion object {
        private const val TAG = "Omalink"
        private const val CHANNEL = "link"
        private const val ASK_CHANNEL = "ask"
        private const val SMS_SENT = "io.omalink.SMS_SENT"
        const val EXTRA_REQUEST = "request"

        @Volatile var instance: LinkService? = null

        fun start(context: Context) {
            // Without the Bluetooth permission there is nothing to listen on,
            // and Android refuses this kind of foreground service anyway.
            if (!Perms.granted(context, Perms.BLUETOOTH)) return
            try {
                context.startForegroundService(Intent(context, LinkService::class.java))
            } catch (e: Exception) {
                Log.w(TAG, "could not start: $e")
            }
        }
    }

    lateinit var data: PhoneData
    val transfers = Transfers(this)
    private val main = Handler(Looper.getMainLooper())
    private val outbox = LinkedBlockingQueue<ByteArray>()
    @Volatile private var running = false
    @Volatile private var server: BluetoothServerSocket? = null
    @Volatile private var socket: BluetoothSocket? = null
    @Volatile var connected = false
        private set
    @Volatile var peer = ""
        private set
    private var ringtone: Ringtone? = null
    private var lastStatus = ""
    private var callNumber = ""

    /** Called on the main thread whenever the connection comes or goes. */
    var onChange: (() -> Unit)? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        data = PhoneData(this)
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Connection", NotificationManager.IMPORTANCE_LOW))
        manager.createNotificationChannel(NotificationChannel(ASK_CHANNEL, "Requests from your computer", NotificationManager.IMPORTANCE_HIGH))
        try {
            if (Build.VERSION.SDK_INT >= 34) {
                startForeground(1, status(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            } else {
                startForeground(1, status())
            }
        } catch (e: Exception) {
            Log.w(TAG, "foreground refused: $e")
            stopSelf()
            return
        }
        running = true
        Thread(::acceptLoop, "omalink-accept").start()
        watch()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

    override fun onDestroy() {
        running = false
        instance = null
        close()
        try { server?.close() } catch (e: Exception) { }
        try { unregisterReceiver(receiver) } catch (e: Exception) { }
        contentResolver.unregisterContentObserver(smsObserver)
        contentResolver.unregisterContentObserver(callObserver)
        super.onDestroy()
    }

    private fun status(): Notification {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val clip = PendingIntent.getActivity(
            this, 1,
            Intent(this, ShareActivity::class.java).setAction(ShareActivity.SEND_CLIPBOARD).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle(if (connected) "Connected to ${peer.ifEmpty { "your computer" }}" else "Waiting for your computer")
            .setContentIntent(open)
            .setOngoing(true)
        if (connected) builder.addAction(Notification.Action.Builder(null, "Send clipboard", clip).build())
        return builder.build()
    }

    private fun refresh() {
        getSystemService(NotificationManager::class.java).notify(1, status())
        main.post { onChange?.invoke() }
    }

    // The connection.

    private fun acceptLoop() {
        val uuid = UUID.fromString(Core.serviceUuid())
        while (running) {
            try {
                val adapter = getSystemService(BluetoothManager::class.java).adapter
                if (adapter == null || !adapter.isEnabled || !Perms.has(this, Manifest.permission.BLUETOOTH_CONNECT) && Build.VERSION.SDK_INT >= 31) {
                    Thread.sleep(5000)
                    continue
                }
                // The secure variant only accepts devices the user has paired.
                val listening = adapter.listenUsingRfcommWithServiceRecord("Omalink", uuid)
                server = listening
                val accepted = listening.accept()
                listening.close()
                server = null
                serve(accepted)
            } catch (e: SecurityException) {
                Thread.sleep(5000)
            } catch (e: Exception) {
                // Bluetooth was switched off mid-listen, most likely.
                if (running) Thread.sleep(3000)
            }
        }
    }

    private fun serve(accepted: BluetoothSocket) {
        socket = accepted
        outbox.clear()
        connected = true
        peer = try { accepted.remoteDevice?.name ?: "" } catch (e: SecurityException) { "" }
        NotifListener.instance?.forgetIcons()
        lastStatus = ""
        refresh()

        val writer = Thread({
            try {
                val out = accepted.outputStream
                while (connected) {
                    out.write(outbox.take())
                    if (outbox.isEmpty()) out.flush()
                }
            } catch (e: Exception) {
                close()
            }
        }, "omalink-write")
        writer.start()

        send(message("hello", JSONObject().put("version", Core.protocolVersion()).put("name", Build.MODEL ?: "Phone")))

        val codec = Core.newCodec()
        try {
            val input = accepted.inputStream
            val buffer = ByteArray(16 * 1024)
            while (connected) {
                val count = input.read(buffer)
                if (count < 0) break
                val messages = Core.feed(codec, buffer, count) ?: break
                for (json in messages) {
                    try {
                        handle(JSONObject(json))
                    } catch (e: Exception) {
                        Log.w(TAG, "request failed: $e")
                    }
                }
            }
        } catch (e: Exception) {
            // The computer went away.
        } finally {
            Core.freeCodec(codec)
            close()
            writer.interrupt()
            refresh()
        }
    }

    private fun close() {
        connected = false
        try { socket?.close() } catch (e: Exception) { }
        socket = null
    }

    /** Queues a message for the computer; dropped when it isn't connected. */
    fun send(json: String) {
        if (!connected) return
        val framed = Core.frame(json)
        if (framed == null) {
            Log.w(TAG, "refused to send: ${Core.explain(json)}")
            return
        }
        outbox.offer(framed)
    }

    fun sendClipboard(text: String) = send(message("clipboard", JSONObject().put("text", text)))

    fun sendPermissions() = send(data.permissions())

    private fun fail(what: String, reason: String) =
        send(message("failed", JSONObject().put("what", what).put("reason", reason)))

    // What the computer asks for.

    private fun handle(request: JSONObject) {
        val d = request.optJSONObject("d") ?: JSONObject()
        when (request.getString("t")) {
            "hello" -> peer = d.optString("name", peer).also { refresh() }
            "ping" -> send(message("pong", JSONObject().put("n", d.optLong("n"))))
            "sync" -> {
                val what = d.optJSONArray("what") ?: JSONArray()
                for (i in 0 until what.length()) sync(what.getString(i))
            }
            "get_messages" -> send(data.messages(d.getLong("thread_id"), d.optInt("limit", 200)))
            "send_sms" -> sendSms(d.getLong("id"), d.getString("to"), d.getString("body"))
            "mark_read" -> markRead(d.getLong("thread_id"))
            "send_file" -> transfers.receive(d.getLong("id"), d.optString("name", "file"), d.optLong("size", -1))
            "get_contact_photos" -> {
                val ids = d.optJSONArray("ids") ?: JSONArray()
                sendPhotos((0 until ids.length()).map(ids::getLong))
            }
            "dismiss_notification" -> NotifListener.instance?.dismiss(d.getString("key"))
            "notification_action" -> {
                if (NotifListener.instance?.press(d.getString("key"), d.getInt("index")) != true)
                    fail("notification_action", "That notification is gone.")
            }
            "reply_notification" -> {
                if (NotifListener.instance?.reply(d.getString("key"), d.getString("text")) != true)
                    fail("reply_notification", "That notification can no longer be replied to.")
            }
            "dial" -> dial(d.getString("number"))
            "answer_call" -> call("answer_call") { it.acceptRingingCall() }
            "end_call" -> call("end_call") { it.endCall() }
            "find_phone" -> main.post(::ring)
            "clipboard" -> main.post {
                getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("From computer", d.getString("text")))
            }
            "request_permissions" -> {
                val names = d.optJSONArray("permissions") ?: JSONArray()
                ask(ArrayList<String>().apply { for (i in 0 until names.length()) add(names.getString(i)) })
            }
        }
    }

    /**
     * Android lets only the default SMS app change what is read, so this
     * presses "Mark as read" on that app's own notification for the
     * conversation. Unread messages whose notification is already gone
     * stay unread on the phone.
     */
    private fun markRead(thread: Long) {
        val listener = NotifListener.instance ?: return
        val smsApp = Telephony.Sms.getDefaultSmsPackage(this) ?: return
        val (address, bodies) = data.unread(thread)
        if (bodies.isEmpty()) return
        val sender = Core.numberKey(address)
        val name = data.nameFor(address)
        listener.markRead(smsApp) { title, text ->
            title.equals(name, ignoreCase = true) && name.isNotEmpty() ||
                title.isNotEmpty() && Core.numberKey(title) == sender ||
                text.length >= 12 && bodies.any { it.startsWith(text) }
        }
    }

    private fun sync(what: String) {
        when (what) {
            "status" -> send(data.status().also { lastStatus = it })
            "permissions" -> send(data.permissions())
            "notifications" -> NotifListener.instance?.let { send(it.all()) }
            "contacts" -> if (Perms.granted(this, Perms.CONTACTS)) send(data.contacts())
            "conversations" -> if (Perms.granted(this, Perms.SMS)) send(data.conversations())
            "call_log" -> if (Perms.granted(this, Perms.CALL_LOG)) send(data.callLog())
            // Decoding and shrinking it takes a moment; don't hold up requests.
            "wallpaper" -> if (Perms.granted(this, Perms.WALLPAPER)) Thread { data.wallpaper()?.let(::send) }.start()
        }
    }

    /** A few at a time, so a long address book doesn't sit in front of a new text. */
    private fun sendPhotos(ids: List<Long>) {
        if (!Perms.granted(this, Perms.CONTACTS)) return
        Thread {
            for (batch in ids.take(4000).chunked(20)) {
                if (!connected) break
                send(data.contactPhotos(batch))
            }
        }.start()
    }

    private fun sendSms(id: Long, to: String, body: String) {
        fun result(ok: Boolean, error: String = "") =
            send(message("sms_sent", JSONObject().put("id", id).put("ok", ok).put("error", error)))
        if (!Perms.has(this, Manifest.permission.SEND_SMS)) {
            return result(false, "Sending messages isn't allowed on the phone yet.")
        }
        try {
            val manager = if (Build.VERSION.SDK_INT >= 31) getSystemService(SmsManager::class.java) else @Suppress("DEPRECATION") SmsManager.getDefault()
            val parts = manager.divideMessage(body)
            // The radio reports back per part; the last part's result stands
            // for the message.
            val sent = ArrayList<PendingIntent?>()
            for (i in parts.indices) {
                sent.add(if (i == parts.size - 1) PendingIntent.getBroadcast(
                    this, id.toInt(),
                    Intent(SMS_SENT).setPackage(packageName).putExtra("id", id),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ) else null)
            }
            manager.sendMultipartTextMessage(to, null, parts, sent, null)
        } catch (e: Exception) {
            result(false, "The phone couldn't send the message.")
        }
    }

    private fun dial(number: String) {
        if (!Perms.has(this, Manifest.permission.CALL_PHONE)) return fail("dial", "Placing calls isn't allowed on the phone yet.")
        try {
            getSystemService(TelecomManager::class.java).placeCall(Uri.fromParts("tel", number, null), null)
        } catch (e: Exception) {
            fail("dial", "The phone couldn't place the call.")
        }
    }

    private fun call(what: String, action: (TelecomManager) -> Unit) {
        if (!Perms.has(this, Manifest.permission.ANSWER_PHONE_CALLS)) return fail(what, "Handling calls isn't allowed on the phone yet.")
        try {
            @Suppress("DEPRECATION")
            action(getSystemService(TelecomManager::class.java))
        } catch (e: Exception) {
            fail(what, "The phone couldn't do that.")
        }
    }

    /** Rings at full alarm volume for a while, even on silent, so the phone can be found. */
    private fun ring() {
        ringtone?.stop()
        val audio = getSystemService(AudioManager::class.java)
        val previous = audio.getStreamVolume(AudioManager.STREAM_ALARM)
        audio.setStreamVolume(AudioManager.STREAM_ALARM, audio.getStreamMaxVolume(AudioManager.STREAM_ALARM), 0)
        val tone = RingtoneManager.getRingtone(this, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM) ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE))
        tone?.audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build()
        tone?.play()
        ringtone = tone
        main.postDelayed({
            tone?.stop()
            audio.setStreamVolume(AudioManager.STREAM_ALARM, previous, 0)
        }, 12000)
    }

    /**
     * The computer wants access the user hasn't given. An app can't open
     * itself from the background, so a notification does the asking; if the
     * app is already on screen it prompts right away.
     */
    private fun ask(names: ArrayList<String>) {
        val missing = ArrayList(names.filter { it in Perms.all && !Perms.granted(this, it) })
        if (missing.isEmpty()) return sendPermissions()
        val intent = Intent(this, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putStringArrayListExtra(EXTRA_REQUEST, missing)
        val labels = missing.mapNotNull { Perms.labels[it]?.first }.joinToString(", ")
        val notification = Notification.Builder(this, ASK_CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle("Your computer needs access")
            .setContentText("Tap to allow: $labels")
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(this, 2, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
            .build()
        getSystemService(NotificationManager::class.java).notify(2, notification)
        try { startActivity(intent) } catch (e: Exception) { }
    }

    // What the phone tells the computer unasked.

    /** Many rows change at once when a message arrives; report once it settles. */
    private fun debounced(action: () -> Unit): ContentObserver {
        val run = Runnable { Thread(action).start() }
        return object : ContentObserver(main) {
            override fun onChange(selfChange: Boolean) {
                main.removeCallbacks(run)
                main.postDelayed(run, 600)
            }
        }
    }

    private val smsObserver = debounced { if (connected && Perms.granted(this, Perms.SMS)) send(data.conversations()) }
    private val callObserver = debounced { if (connected && Perms.granted(this, Perms.CALL_LOG)) send(data.callLog()) }

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                Intent.ACTION_BATTERY_CHANGED -> {
                    // Fires for every voltage wobble; only tell the computer
                    // when something it shows has changed.
                    if (!connected) return
                    val now = data.status()
                    if (now != lastStatus) {
                        lastStatus = now
                        send(now)
                    }
                }
                TelephonyManager.ACTION_PHONE_STATE_CHANGED -> {
                    // Sent twice, once with the number and once without.
                    @Suppress("DEPRECATION")
                    intent.getStringExtra(TelephonyManager.EXTRA_INCOMING_NUMBER)?.let { if (it.isNotEmpty()) callNumber = it }
                    val phase = when (intent.getStringExtra(TelephonyManager.EXTRA_STATE)) {
                        TelephonyManager.EXTRA_STATE_RINGING -> "ringing"
                        TelephonyManager.EXTRA_STATE_OFFHOOK -> "active"
                        else -> "idle"
                    }
                    send(message("call_state", JSONObject().put("phase", phase).put("number", callNumber).put("name", data.nameFor(callNumber))))
                    if (phase == "idle") callNumber = ""
                }
                SMS_SENT -> {
                    val ok = resultCode == android.app.Activity.RESULT_OK
                    send(message("sms_sent", JSONObject()
                        .put("id", intent.getLongExtra("id", 0))
                        .put("ok", ok)
                        .put("error", if (ok) "" else "The network didn't accept the message.")))
                }
            }
        }
    }

    private fun watch() {
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_BATTERY_CHANGED)
            addAction(TelephonyManager.ACTION_PHONE_STATE_CHANGED)
            addAction(SMS_SENT)
        }
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED) else registerReceiver(receiver, filter)
        // Registering needs no permission; nothing is delivered until the
        // matching read permission is granted.
        try { contentResolver.registerContentObserver(Uri.parse("content://sms"), true, smsObserver) } catch (e: Exception) { }
        try { contentResolver.registerContentObserver(CallLog.Calls.CONTENT_URI, true, callObserver) } catch (e: Exception) { }
    }
}
