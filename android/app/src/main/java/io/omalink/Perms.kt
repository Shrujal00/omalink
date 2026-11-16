package io.omalink

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Environment
import android.os.PowerManager
import android.provider.Settings

/**
 * The things the user has to allow, under the names the desktop knows them
 * by. Most map to Android runtime permissions; the rest are switches in Settings.
 */
object Perms {
    const val BLUETOOTH = "bluetooth"
    const val POST_NOTIFICATIONS = "post_notifications"
    const val NOTIFICATIONS = "notifications"
    const val SMS = "sms"
    const val CONTACTS = "contacts"
    const val CALL_LOG = "call_log"
    const val PHONE = "phone"
    const val BATTERY = "battery"
    const val WALLPAPER = "wallpaper"
    const val CONTROL = "control"
    const val PHOTOS = "photos"

    val all = listOf(BLUETOOTH, POST_NOTIFICATIONS, NOTIFICATIONS, SMS, CONTACTS, CALL_LOG, PHONE, PHOTOS, BATTERY, WALLPAPER, CONTROL)

    val labels = mapOf(
        BLUETOOTH to ("Nearby devices" to "Stay connected to your computer"),
        POST_NOTIFICATIONS to ("Show notifications" to "Keep the connection alive in the background"),
        NOTIFICATIONS to ("Notification access" to "Show this phone's notifications on your computer"),
        SMS to ("Messages" to "Read and send text messages"),
        CONTACTS to ("Contacts" to "Show names instead of numbers"),
        CALL_LOG to ("Call history" to "Show recent calls"),
        PHONE to ("Phone" to "Place, answer, and end calls"),
        BATTERY to ("Run in background" to "Stop Android from putting the app to sleep"),
        WALLPAPER to ("Wallpaper" to "Show this phone's wallpaper on your computer. Android asks for all files access to read it"),
        PHOTOS to ("Photos and videos" to "Browse, download and delete your gallery from your computer"),
        CONTROL to ("Control from computer" to "Use the mouse and keyboard on this phone while its screen is shared. Turn on Omalink under Accessibility"),
    )

    /** The runtime permissions behind a name; empty for the Settings switches. */
    fun runtime(name: String): Array<String> = when (name) {
        BLUETOOTH -> if (Build.VERSION.SDK_INT >= 31) arrayOf(Manifest.permission.BLUETOOTH_CONNECT) else emptyArray()
        POST_NOTIFICATIONS -> if (Build.VERSION.SDK_INT >= 33) arrayOf(Manifest.permission.POST_NOTIFICATIONS) else emptyArray()
        SMS -> arrayOf(Manifest.permission.READ_SMS, Manifest.permission.SEND_SMS, Manifest.permission.RECEIVE_SMS)
        CONTACTS -> arrayOf(Manifest.permission.READ_CONTACTS)
        CALL_LOG -> arrayOf(Manifest.permission.READ_CALL_LOG)
        PHONE -> arrayOf(Manifest.permission.READ_PHONE_STATE, Manifest.permission.CALL_PHONE, Manifest.permission.ANSWER_PHONE_CALLS)
        WALLPAPER -> if (Build.VERSION.SDK_INT < 30) arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE) else emptyArray()
        PHOTOS -> if (Build.VERSION.SDK_INT >= 33) arrayOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VIDEO)
                  else arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        else -> emptyArray()
    }

    fun granted(context: Context, name: String): Boolean = when (name) {
        NOTIFICATIONS -> {
            val enabled = Settings.Secure.getString(context.contentResolver, "enabled_notification_listeners") ?: ""
            enabled.split(':').any { it.startsWith(context.packageName + "/") }
        }
        BATTERY -> context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)
        CONTROL -> {
            val enabled = Settings.Secure.getString(context.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: ""
            enabled.split(':').any { it.startsWith(context.packageName + "/") }
        }
        // Since Android 13 the wallpaper is readable only with all files access.
        WALLPAPER -> if (Build.VERSION.SDK_INT >= 30) Environment.isExternalStorageManager() else has(context, Manifest.permission.READ_EXTERNAL_STORAGE)
        else -> runtime(name).all { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }
    }

    fun has(context: Context, permission: String) =
        context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
}
