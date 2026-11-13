package io.omalink

import android.app.Notification
import android.app.RemoteInput
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Bundle
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream

/** Mirrors this phone's notifications to the desktop and acts on them for it. */
class NotifListener : NotificationListenerService() {
    companion object {
        @Volatile var instance: NotifListener? = null
    }

    /** Packages whose icon the desktop already has, for this connection. */
    private val iconsSent = HashSet<String>()

    override fun onListenerConnected() {
        instance = this
        LinkService.instance?.let { it.send(it.data.permissions()); it.send(all()) }
    }

    override fun onListenerDisconnected() {
        instance = null
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        val service = LinkService.instance ?: return
        if (!service.connected) return
        val json = describe(sbn) ?: return
        service.send(message("notification_posted", json))
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification) {
        val service = LinkService.instance ?: return
        if (service.connected) service.send(message("notification_removed", JSONObject().put("key", sbn.key)))
    }

    /** A new connection starts with no icons. */
    fun forgetIcons() = synchronized(iconsSent) { iconsSent.clear() }

    fun all(): String {
        val items = JSONArray()
        val active = try { activeNotifications } catch (e: Exception) { null }
        active?.forEach { sbn -> describe(sbn)?.let(items::put) }
        return message("notifications", JSONObject().put("items", items))
    }

    /** The buttons that are plain buttons, not reply fields. */
    private fun buttons(notification: Notification): List<Notification.Action> =
        notification.actions?.filter { it.remoteInputs.isNullOrEmpty() && it.title != null } ?: emptyList()

    private fun replyAction(notification: Notification): Notification.Action? =
        notification.actions?.firstOrNull { action -> action.remoteInputs?.any { it.allowFreeFormInput } == true }

    private fun describe(sbn: StatusBarNotification): JSONObject? {
        val notification = sbn.notification ?: return null
        if (sbn.packageName == packageName) return null
        // Group summaries repeat their children; ongoing ones (music, timers,
        // navigation) update many times a second and aren't news.
        if (notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) return null
        if (sbn.isOngoing) return null
        val extras = notification.extras ?: Bundle.EMPTY
        val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString() ?: ""
        val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString() ?: ""
        if (title.isEmpty() && text.isEmpty()) return null

        val app = try {
            packageManager.getApplicationLabel(packageManager.getApplicationInfo(sbn.packageName, 0)).toString()
        } catch (e: Exception) {
            sbn.packageName
        }
        val json = JSONObject()
            .put("key", sbn.key)
            .put("package", sbn.packageName)
            .put("app", app)
            .put("title", title)
            .put("text", text.take(2000))
            .put("time", sbn.postTime)
            .put("actions", JSONArray(buttons(notification).map { it.title.toString() }))
            .put("can_reply", replyAction(notification) != null)
            .put("clearable", sbn.isClearable)
        val first = synchronized(iconsSent) { iconsSent.add(sbn.packageName) }
        json.put("icon", if (first) icon(sbn.packageName) else "")
        return json
    }

    private fun icon(pkg: String): String = try {
        val drawable = packageManager.getApplicationIcon(pkg)
        val bitmap = Bitmap.createBitmap(72, 72, Bitmap.Config.ARGB_8888)
        drawable.setBounds(0, 0, 72, 72)
        drawable.draw(Canvas(bitmap))
        val bytes = ByteArrayOutputStream()
        bitmap.compress(Bitmap.CompressFormat.PNG, 100, bytes)
        Base64.encodeToString(bytes.toByteArray(), Base64.NO_WRAP)
    } catch (e: Exception) {
        ""
    }

    private fun find(key: String): StatusBarNotification? =
        try { activeNotifications?.firstOrNull { it.key == key } } catch (e: Exception) { null }

    fun dismiss(key: String) {
        try { cancelNotification(key) } catch (e: Exception) { }
    }

    fun press(key: String, index: Int): Boolean {
        val action = find(key)?.notification?.let(::buttons)?.getOrNull(index) ?: return false
        return try { action.actionIntent?.send(); true } catch (e: Exception) { false }
    }

    /**
     * Presses "Mark as read" on each of `pkg`'s notifications that `same`
     * recognises by its title and text.
     */
    fun markRead(pkg: String, same: (title: String, text: String) -> Boolean) {
        val active = try { activeNotifications } catch (e: Exception) { null } ?: return
        for (sbn in active) {
            val notification = sbn.notification ?: continue
            if (sbn.packageName != pkg || notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) continue
            val extras = notification.extras ?: continue
            val title = (extras.getCharSequence(Notification.EXTRA_CONVERSATION_TITLE) ?: extras.getCharSequence(Notification.EXTRA_TITLE))?.toString()?.trim() ?: ""
            val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString()?.trim() ?: ""
            if (!same(title, text)) continue
            val actions = notification.actions ?: continue
            val action = actions.firstOrNull { it.semanticAction == Notification.Action.SEMANTIC_ACTION_MARK_AS_READ }
                ?: actions.firstOrNull { it.remoteInputs.isNullOrEmpty() && it.title?.toString()?.contains("read", ignoreCase = true) == true }
                ?: continue
            try { action.actionIntent?.send() } catch (e: Exception) { }
        }
    }

    fun reply(key: String, text: String): Boolean {
        val action = find(key)?.notification?.let(::replyAction) ?: return false
        val inputs = action.remoteInputs ?: return false
        val results = Bundle()
        for (input in inputs) results.putCharSequence(input.resultKey, text)
        val intent = Intent().addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        RemoteInput.addResultsToIntent(inputs, intent, results)
        return try { action.actionIntent?.send(this, 0, intent); true } catch (e: Exception) { false }
    }
}
