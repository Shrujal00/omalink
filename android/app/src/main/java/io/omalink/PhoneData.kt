package io.omalink

import android.app.WallpaperManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.Cursor
import android.graphics.Bitmap
import android.graphics.Canvas
import android.media.AudioManager
import android.net.Uri
import android.os.BatteryManager
import android.provider.CallLog
import android.provider.ContactsContract
import android.telephony.TelephonyManager
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream

/** Builds one protocol message: `{"t": type, "d": data}`. */
fun message(type: String, data: JSONObject? = null): String {
    val json = JSONObject().put("t", type)
    if (data != null) json.put("d", data)
    return json.toString()
}

/** Reads what the desktop asks for out of Android's content providers. */
class PhoneData(private val context: Context) {
    private val smsUri: Uri = Uri.parse("content://sms")

    /** Number key → contact name, rebuilt whenever contacts are read. */
    @Volatile private var names: Map<String, String> = emptyMap()

    private fun <T> query(uri: Uri, columns: Array<String>, where: String?, args: Array<String>?, order: String?, read: (Cursor) -> T): T? =
        try {
            context.contentResolver.query(uri, columns, where, args, order)?.use(read)
        } catch (e: Exception) {
            // A revoked permission or an OEM's unusual provider; the desktop
            // shows what it has rather than nothing.
            null
        }

    fun nameFor(number: String): String = names[Core.numberKey(number)] ?: ""

    fun status(): String {
        val battery = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = battery?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
        val plugged = (battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0
        val telephony = context.getSystemService(TelephonyManager::class.java)
        val signal = try { telephony.signalStrength?.level ?: -1 } catch (e: Exception) { -1 }
        val ringer = when (context.getSystemService(AudioManager::class.java).ringerMode) {
            AudioManager.RINGER_MODE_SILENT -> "silent"
            AudioManager.RINGER_MODE_VIBRATE -> "vibrate"
            else -> "normal"
        }
        return message("status", JSONObject()
            .put("battery", if (level >= 0 && scale > 0) level * 100 / scale else -1)
            .put("charging", plugged)
            .put("signal", signal)
            .put("carrier", telephony.networkOperatorName ?: "")
            .put("ringer", ringer))
    }

    fun permissions(): String {
        val granted = JSONArray()
        val missing = JSONArray()
        for (name in Perms.all) (if (Perms.granted(context, name)) granted else missing).put(name)
        return message("permissions", JSONObject().put("granted", granted).put("missing", missing))
    }

    fun contacts(): String {
        val byName = LinkedHashMap<String, JSONArray>()
        val photos = HashMap<String, Long>()
        val lookup = HashMap<String, String>()
        query(
            ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
            arrayOf(ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME, ContactsContract.CommonDataKinds.Phone.NUMBER, ContactsContract.CommonDataKinds.Phone.PHOTO_ID),
            null, null, ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME + " COLLATE LOCALIZED ASC",
        ) { cursor ->
            val seen = HashSet<String>()
            while (cursor.moveToNext() && lookup.size < 8000) {
                val name = cursor.getString(0) ?: continue
                val number = cursor.getString(1) ?: continue
                val photo = cursor.getLong(2)
                if (photo != 0L) photos.putIfAbsent(name, photo)
                val key = Core.numberKey(number)
                // The same number synced from two accounts appears twice.
                if (!seen.add("$name|$key")) continue
                byName.getOrPut(name) { JSONArray() }.put(number)
                lookup.putIfAbsent(key, name)
            }
        }
        names = lookup
        val items = JSONArray()
        for ((name, numbers) in byName) items.put(JSONObject().put("name", name).put("numbers", numbers).put("photo", photos[name] ?: 0L))
        return message("contacts", JSONObject().put("items", items))
    }

    /** The thumbnails Android keeps for these photo ids, as one message. */
    fun contactPhotos(ids: List<Long>): String {
        val items = JSONArray()
        val list = ids.joinToString(",")
        query(ContactsContract.Data.CONTENT_URI, arrayOf(ContactsContract.Data._ID, ContactsContract.CommonDataKinds.Photo.PHOTO), "${ContactsContract.Data._ID} IN ($list)", null, null) { cursor ->
            while (cursor.moveToNext()) {
                val bytes = cursor.getBlob(1) ?: continue
                items.put(JSONObject().put("id", cursor.getLong(0)).put("data", Base64.encodeToString(bytes, Base64.NO_WRAP)))
            }
        }
        return message("contact_photos", JSONObject().put("items", items))
    }

    /**
     * The home screen wallpaper, cropped to the screen's shape and shrunk;
     * null when Android won't hand it over.
     */
    fun wallpaper(): String? = try {
        val drawable = WallpaperManager.getInstance(context).drawable!!
        val metrics = context.resources.displayMetrics
        val (screenW, screenH) = minOf(metrics.widthPixels, metrics.heightPixels) to maxOf(metrics.widthPixels, metrics.heightPixels)
        val (sourceW, sourceH) = drawable.intrinsicWidth to drawable.intrinsicHeight
        if (sourceW <= 0 || sourceH <= 0) {
            null
        } else {
            val height = 720
            val width = height * screenW / screenH
            // Fill the frame and centre, as the launcher does.
            val scale = maxOf(width.toFloat() / sourceW, height.toFloat() / sourceH)
            val (drawW, drawH) = (sourceW * scale).toInt() to (sourceH * scale).toInt()
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            drawable.setBounds((width - drawW) / 2, (height - drawH) / 2, (width + drawW) / 2, (height + drawH) / 2)
            drawable.draw(Canvas(bitmap))
            val bytes = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, 85, bytes)
            message("wallpaper", JSONObject().put("image", Base64.encodeToString(bytes.toByteArray(), Base64.NO_WRAP)))
        }
    } catch (e: Exception) {
        null
    }

    /**
     * One row per thread, newest first. Grouping is done here rather than
     * through the conversations provider, whose columns differ between
     * manufacturers.
     */
    fun conversations(): String {
        if (names.isEmpty() && Perms.granted(context, Perms.CONTACTS)) contacts()
        val threads = LinkedHashMap<Long, JSONObject>()
        query(smsUri, arrayOf("thread_id", "address", "body", "date", "type", "read"), null, null, "date DESC") { cursor ->
            var rows = 0
            while (cursor.moveToNext() && rows++ < 5000) {
                val thread = cursor.getLong(0)
                val unread = cursor.getInt(4) == 1 && cursor.getInt(5) == 0
                val existing = threads[thread]
                if (existing != null) {
                    if (unread) existing.put("unread", existing.getInt("unread") + 1)
                    continue
                }
                if (threads.size >= 150) continue
                val address = cursor.getString(1) ?: ""
                threads[thread] = JSONObject()
                    .put("thread_id", thread)
                    .put("address", address)
                    .put("name", nameFor(address))
                    .put("snippet", (cursor.getString(2) ?: "").take(160))
                    .put("time", cursor.getLong(3))
                    .put("unread", if (unread) 1 else 0)
            }
        }
        return message("conversations", JSONObject().put("items", JSONArray(threads.values)))
    }

    /** The newest `limit` messages of a thread, oldest first. */
    fun messages(thread: Long, limit: Int): String {
        val rows = ArrayList<JSONObject>()
        query(smsUri, arrayOf("_id", "address", "body", "date", "type", "read"), "thread_id = ?", arrayOf(thread.toString()), "date DESC") { cursor ->
            while (cursor.moveToNext() && rows.size < limit) {
                rows.add(JSONObject()
                    .put("id", cursor.getLong(0))
                    .put("thread_id", thread)
                    .put("address", cursor.getString(1) ?: "")
                    .put("body", cursor.getString(2) ?: "")
                    .put("time", cursor.getLong(3))
                    // Type 1 is the inbox; everything else was written here.
                    .put("outgoing", cursor.getInt(4) != 1)
                    .put("read", cursor.getInt(5) != 0))
            }
        }
        rows.reverse()
        return message("messages", JSONObject().put("thread_id", thread).put("items", JSONArray(rows)))
    }

    /** Who a thread is with, and the unread messages received in it. */
    fun unread(thread: Long): Pair<String, List<String>> {
        var address = ""
        val bodies = ArrayList<String>()
        query(smsUri, arrayOf("address", "body"), "thread_id = ? AND type = 1 AND read = 0", arrayOf(thread.toString()), "date DESC") { cursor ->
            while (cursor.moveToNext() && bodies.size < 50) {
                if (address.isEmpty()) address = cursor.getString(0) ?: ""
                bodies.add(cursor.getString(1) ?: "")
            }
        }
        return address to bodies
    }

    fun callLog(): String {
        val items = JSONArray()
        query(
            CallLog.Calls.CONTENT_URI,
            arrayOf(CallLog.Calls._ID, CallLog.Calls.NUMBER, CallLog.Calls.CACHED_NAME, CallLog.Calls.TYPE, CallLog.Calls.DATE, CallLog.Calls.DURATION),
            null, null, CallLog.Calls.DATE + " DESC",
        ) { cursor ->
            while (cursor.moveToNext() && items.length() < 200) {
                val number = cursor.getString(1) ?: ""
                val kind = when (cursor.getInt(3)) {
                    CallLog.Calls.OUTGOING_TYPE -> "outgoing"
                    CallLog.Calls.MISSED_TYPE -> "missed"
                    CallLog.Calls.REJECTED_TYPE, CallLog.Calls.BLOCKED_TYPE -> "rejected"
                    else -> "incoming"
                }
                items.put(JSONObject()
                    .put("id", cursor.getLong(0))
                    .put("number", number)
                    .put("name", cursor.getString(2) ?: nameFor(number))
                    .put("kind", kind)
                    .put("time", cursor.getLong(4))
                    .put("duration", cursor.getLong(5)))
            }
        }
        return message("call_log", JSONObject().put("items", items))
    }
}
