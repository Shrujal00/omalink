package io.omalink

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import org.json.JSONObject

/**
 * Shares the screen with the computer while a verified session lasts: the
 * screen goes into a hardware H.264 encoder, and what comes out goes on a
 * stream of its own on the network link (see `MirrorStream` in the protocol).
 *
 * Stops when the computer says so, when the person taps Stop in the
 * notification or Android's own indicator, or when the link drops.
 */
class MirrorService : Service() {
    companion object {
        private const val TAG = "Omalink"
        private const val CHANNEL = "mirror"
        private const val STOP = "io.omalink.STOP_MIRROR"
        private const val EXTRA_RESULT = "result"
        private const val EXTRA_DATA = "data"
        private const val EXTRA_SIZE = "max_size"

        @Volatile var instance: MirrorService? = null
        /** A verified session is on; only then does the computer get to touch. */
        val active get() = instance?.projection != null

        fun start(context: Context, result: Int, data: Intent, maxSize: Int) {
            val intent = Intent(context, MirrorService::class.java)
                .putExtra(EXTRA_RESULT, result)
                .putExtra(EXTRA_DATA, data)
                .putExtra(EXTRA_SIZE, maxSize)
            try { context.startForegroundService(intent) } catch (e: Exception) { Log.w(TAG, "mirror refused: $e") }
        }
    }

    private val main = Handler(Looper.getMainLooper())
    @Volatile private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    private var codec: MediaCodec? = null
    @Volatile private var handle = 0L
    @Volatile private var running = false
    private var maxSize = 1600
    private var width = 0
    private var height = 0
    private var encoder: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) {
            stop("Stopped on the phone.")
            return START_NOT_STICKY
        }
        val result = intent?.getIntExtra(EXTRA_RESULT, 0) ?: 0
        @Suppress("DEPRECATION")
        val data = intent?.getParcelableExtra<Intent>(EXTRA_DATA)
        maxSize = (intent?.getIntExtra(EXTRA_SIZE, 1600) ?: 1600).coerceIn(480, 2560)
        if (data == null || projection != null) return START_NOT_STICKY

        // Android wants the foreground service up before the projection exists.
        try {
            val notification = notification()
            if (Build.VERSION.SDK_INT >= 29) startForeground(5, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
            else startForeground(5, notification)
        } catch (e: Exception) {
            Log.w(TAG, "mirror foreground refused: $e")
            stopSelf()
            return START_NOT_STICKY
        }
        val manager = getSystemService(MediaProjectionManager::class.java)
        val granted = try { manager.getMediaProjection(result, data) } catch (e: Exception) { null }
        if (granted == null) {
            stop("Android didn't allow screen sharing.")
            return START_NOT_STICKY
        }
        projection = granted
        instance = this
        granted.registerCallback(object : MediaProjection.Callback() {
            override fun onStop() { main.post { stop("Stopped on the phone.") } }
        }, main)
        if (!begin()) stop("The phone couldn't encode its screen.")
        return START_NOT_STICKY
    }

    private fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Screen sharing", NotificationManager.IMPORTANCE_LOW))
        val stop = PendingIntent.getService(this, 6, Intent(this, MirrorService::class.java).setAction(STOP), PendingIntent.FLAG_IMMUTABLE)
        val name = LinkService.instance?.peer?.ifEmpty { null } ?: "your computer"
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle("Sharing your screen with $name")
            .setContentText(if (ControlService.instance != null) "$name can also control this phone" else "View only")
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Stop", stop).build())
            .build()
    }

    /** The screen's size, scaled so its longer edge fits the computer's ask. */
    private fun size(): Pair<Int, Int> {
        val (w, h) = screenSize(this)
        val scale = minOf(1.0, maxSize.toDouble() / maxOf(w, h))
        // Encoders want dimensions in whole macroblocks.
        fun fit(v: Int) = maxOf(16, (v * scale).toInt() / 16 * 16)
        return fit(w) to fit(h)
    }

    /** Starts the encoder and a fresh video stream at the current size. */
    private fun begin(): Boolean {
        val (w, h) = size()
        width = w
        height = h
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            // About 0.08 bits a pixel a frame at 60 fps, within reason.
            setInteger(MediaFormat.KEY_BIT_RATE, (w * h * 5).coerceIn(2_000_000, 12_000_000))
            setInteger(MediaFormat.KEY_FRAME_RATE, 60)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
            // A still screen still sends a frame now and then, so a decoder
            // that joins late gets a picture.
            setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 100_000)
            setInteger(MediaFormat.KEY_PRIORITY, 0)
            if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
        }
        val codec = try {
            MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
                configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            }
        } catch (e: Exception) {
            Log.w(TAG, "encoder: $e")
            return false
        }
        val surface = codec.createInputSurface()
        codec.start()
        this.codec = codec
        handle = Core.netOpenMirror(w, h)
        if (handle == 0L) return false
        val dpi = resources.displayMetrics.densityDpi
        display = projection?.createVirtualDisplay("Omalink", w, h, dpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, surface, null, null)
        LinkService.instance?.send(message("control_state", JSONObject().put("available", ControlService.instance != null)))
        running = true
        encoder = Thread({ drain(codec, handle) }, "omalink-encode").apply { start() }
        return true
    }

    /** Hands each encoded frame to the network until the session ends. */
    private fun drain(codec: MediaCodec, stream: Long) {
        val info = MediaCodec.BufferInfo()
        var bytes = ByteArray(256 * 1024)
        while (running && stream == handle) {
            val index = try { codec.dequeueOutputBuffer(info, 100_000) } catch (e: Exception) { break }
            if (index < 0) continue
            val buffer = codec.getOutputBuffer(index)
            if (buffer != null && info.size > 0) {
                if (bytes.size < info.size) bytes = ByteArray(info.size * 2)
                buffer.position(info.offset)
                buffer.get(bytes, 0, info.size)
                var flags = 0
                if (info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0) flags = flags or 1
                if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) flags = flags or 2
                if (!Core.netWriteFrame(stream, bytes, info.size, info.presentationTimeUs, flags)) {
                    codec.releaseOutputBuffer(index, false)
                    main.post { stop("The connection to the computer was lost.") }
                    break
                }
            }
            codec.releaseOutputBuffer(index, false)
        }
    }

    /** Ends the encoder and its stream, leaving the projection for a restart. */
    private fun end() {
        running = false
        val stream = handle
        handle = 0L
        try { encoder?.join(500) } catch (e: Exception) { }
        encoder = null
        try { display?.release() } catch (e: Exception) { }
        display = null
        try { codec?.stop() } catch (e: Exception) { }
        try { codec?.release() } catch (e: Exception) { }
        codec = null
        if (stream != 0L) Core.netFinish(stream)
    }

    /** The computer's decoder lost its place. */
    fun keyframe() {
        try { codec?.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) } catch (e: Exception) { }
    }

    /** Turning the phone swaps the edges: a new stream at the new size. */
    override fun onConfigurationChanged(config: Configuration) {
        super.onConfigurationChanged(config)
        if (projection == null) return
        val (w, h) = size()
        if (w == width && h == height) return
        end()
        if (!begin()) stop("The phone couldn't encode its screen.")
    }

    fun stop(reason: String) {
        val wasSharing = projection != null
        end()
        try { projection?.stop() } catch (e: Exception) { }
        projection = null
        if (instance === this) instance = null
        if (wasSharing) LinkService.instance?.send(message("mirror_ended", JSONObject().put("reason", reason)))
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        if (projection != null) stop("Stopped on the phone.")
        super.onDestroy()
    }
}
