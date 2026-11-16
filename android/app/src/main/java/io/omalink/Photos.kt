package io.omalink

import android.app.Activity
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ContentResolver
import android.content.ContentUris
import android.content.Intent
import android.database.ContentObserver
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Base64
import android.util.Log
import android.util.Size
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream

/**
 * The phone's gallery, for the computer: pages of photos and videos with
 * small thumbnails, the originals on request, and deleting, which Android
 * makes the person confirm on the phone.
 */
class Photos(private val service: LinkService) {
    private companion object {
        const val TAG = "Omalink"
        const val DELETE_NOTIFICATION = 7
    }

    private val main = Handler(Looper.getMainLooper())

    private fun uriFor(id: Long, video: Boolean): Uri =
        ContentUris.withAppendedId(if (video) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id)

    /** Whether a gallery item is a video, from MediaStore itself. */
    private fun isVideo(id: Long): Boolean = try {
        service.contentResolver.query(
            MediaStore.Files.getContentUri("external"), arrayOf(MediaStore.Files.FileColumns.MEDIA_TYPE),
            "${MediaStore.MediaColumns._ID} = ?", arrayOf(id.toString()), null,
        )?.use { c -> c.moveToFirst() && c.getInt(0) == MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO } ?: false
    } catch (e: Exception) { false }

    /** One page, newest first, older than `before` (milliseconds), as a `photos` message. */
    fun page(limit: Int, before: Long): String {
        val items = JSONArray()
        var cursorAt = 0L
        var count = 0
        val files = MediaStore.Files.getContentUri("external")
        val type = MediaStore.Files.FileColumns.MEDIA_TYPE
        var selection = "($type = ${MediaStore.Files.FileColumns.MEDIA_TYPE_IMAGE} OR $type = ${MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO})"
        val args = ArrayList<String>()
        if (before > 0) {
            selection += " AND ${MediaStore.MediaColumns.DATE_ADDED} < ?"
            args.add((before / 1000).toString())
        }
        val query = Bundle().apply {
            putString(ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
            putStringArray(ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, args.toTypedArray())
            putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, "${MediaStore.MediaColumns.DATE_ADDED} DESC")
            // One more than asked, to know whether there's another page.
            putInt(ContentResolver.QUERY_ARG_LIMIT, limit + 1)
        }
        val columns = arrayOf(
            MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME, type,
            MediaStore.MediaColumns.DATE_TAKEN, MediaStore.MediaColumns.DATE_ADDED, MediaStore.MediaColumns.SIZE,
            MediaStore.MediaColumns.WIDTH, MediaStore.MediaColumns.HEIGHT, MediaStore.MediaColumns.DURATION,
        )
        try {
            service.contentResolver.query(files, columns, query, null)?.use { c ->
                while (c.moveToNext()) {
                    count++
                    if (count > limit) break
                    val id = c.getLong(0)
                    val video = c.getInt(2) == MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO
                    val added = c.getLong(4) * 1000
                    val taken = if (c.isNull(3)) 0L else c.getLong(3)
                    cursorAt = added
                    items.put(JSONObject()
                        .put("id", id)
                        .put("name", c.getString(1) ?: "")
                        .put("kind", if (video) "video" else "image")
                        .put("time", if (taken > 0) taken else added)
                        .put("size", c.getLong(5))
                        .put("width", c.getInt(6))
                        .put("height", c.getInt(7))
                        .put("duration_ms", if (video && !c.isNull(8)) c.getLong(8) else 0L)
                        .put("thumb", thumbnail(uriFor(id, video))))
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "gallery: $e")
        }
        return message("photos", JSONObject()
            .put("items", items)
            .put("more", count > limit)
            .put("cursor", cursorAt)
            .put("before", before))
    }

    private fun thumbnail(uri: Uri): String = try {
        val bitmap = service.contentResolver.loadThumbnail(uri, Size(256, 256), null)
        val out = ByteArrayOutputStream()
        bitmap.compress(Bitmap.CompressFormat.JPEG, 72, out)
        bitmap.recycle()
        Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
    } catch (e: Exception) {
        ""
    }

    /** Sends one original to the computer, the way a shared file goes. */
    fun send(id: Long) {
        val uri = uriFor(id, isVideo(id))
        if (!service.transfers.offer(service, uri)) {
            service.send(message("failed", JSONObject().put("what", "get_photo").put("reason", "That photo isn't on the phone any more.")))
        }
    }

    /**
     * Android only deletes media after the person agrees on the phone, so
     * this puts the question in front of them: straight away when the app is
     * open, otherwise as a notification.
     */
    fun delete(ids: List<Long>) {
        if (ids.isEmpty()) return
        val uris = ids.map { id -> uriFor(id, isVideo(id)) }
        if (Build.VERSION.SDK_INT < 30) {
            // Android 10: no batch request; the computer is told it can't.
            service.send(message("failed", JSONObject().put("what", "delete_photos").put("reason", "Deleting from the computer needs Android 11 or newer.")))
            return
        }
        val request = MediaStore.createDeleteRequest(service.contentResolver, uris)
        val open = Intent(service, ConfirmActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .putExtra(ConfirmActivity.EXTRA_REQUEST, request)
        val notification = Notification.Builder(service, LinkService.ASK_CHANNEL)
            .setSmallIcon(R.drawable.ic_link)
            .setContentTitle(if (ids.size == 1) "Delete 1 item from your phone?" else "Delete ${ids.size} items from your phone?")
            .setContentText("Your computer asked. Tap to confirm.")
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(service, 8, open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
            .build()
        service.getSystemService(NotificationManager::class.java).notify(DELETE_NOTIFICATION, notification)
        try { service.startActivity(open) } catch (e: Exception) { }
    }

    /** Tells the computer the gallery changed, once things settle. */
    private val changed = Runnable { if (service.connected && Perms.granted(service, Perms.PHOTOS)) service.send(message("photos_changed")) }
    val observer = object : ContentObserver(main) {
        override fun onChange(selfChange: Boolean) {
            main.removeCallbacks(changed)
            main.postDelayed(changed, 1500)
        }
    }
}

/** Shows Android's own confirmation for a request the computer made. */
class ConfirmActivity : Activity() {
    companion object {
        const val EXTRA_REQUEST = "request"
        private const val ASK = 11
    }

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        if (saved != null) return
        @Suppress("DEPRECATION")
        val request = intent.getParcelableExtra<PendingIntent>(EXTRA_REQUEST)
        if (request == null) return finish()
        try {
            startIntentSenderForResult(request.intentSender, ASK, null, 0, 0, 0)
        } catch (e: Exception) {
            finish()
        }
    }

    override fun onActivityResult(code: Int, result: Int, data: Intent?) {
        super.onActivityResult(code, result, data)
        getSystemService(NotificationManager::class.java).cancel(7)
        finish()
    }
}
