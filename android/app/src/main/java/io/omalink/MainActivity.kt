package io.omalink

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * The whole interface: whether the computer is connected, and what the user
 * has allowed. Everything else happens on the computer.
 */
class MainActivity : Activity() {
    private val backdrop = Color.parseColor("#16171B")
    private val surface = Color.parseColor("#1F2126")
    private val foreground = Color.parseColor("#E6E8EC")
    private val secondary = Color.parseColor("#A4A9B3")
    private val accent = Color.parseColor("#9ECE6A")
    private val green = Color.parseColor("#7FC27A")

    private lateinit var list: LinearLayout
    private lateinit var state: TextView
    private lateinit var hint: TextView

    /** Settings switches the computer asked for, opened one per return to the app. */
    private val queue = ArrayDeque<String>()

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()

    private fun text(value: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        text = value
        textSize = size
        setTextColor(color)
        if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    }

    private fun rounded(color: Int, radius: Int) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = dp(radius).toFloat()
    }

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(28), dp(20), dp(28))
        }
        column.addView(text("Omalink", 28f, foreground, true))
        column.addView(text("FOR OMARCHY", 12f, accent, true).apply {
            typeface = Typeface.MONOSPACE
            letterSpacing = 0.18f
            setPadding(0, dp(2), 0, dp(6))
        })
        state = text("", 15f, secondary).apply { setPadding(0, dp(6), 0, 0) }
        column.addView(state)
        hint = text("", 14f, secondary).apply { setPadding(0, dp(14), 0, dp(4)); setLineSpacing(0f, 1.2f) }
        column.addView(hint)
        column.addView(text("Permissions", 13f, secondary, true).apply { setPadding(0, dp(22), 0, dp(10)) })
        list = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(list, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        column.addView(text(
            "If Android greys out Notification access or Messages for this app, open App info → ⋮ → Allow restricted settings. Android does that for apps installed outside the Play Store.",
            12.5f, secondary,
        ).apply { setPadding(0, dp(18), 0, 0); setLineSpacing(0f, 1.2f) })

        setContentView(ScrollView(this).apply {
            setBackgroundColor(backdrop)
            isFillViewport = true
            fitsSystemWindows = true
            addView(column)
        })
        accept(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        accept(intent)
    }

    /** Requests forwarded from the computer arrive as an extra on the intent. */
    private fun accept(intent: Intent?) {
        val names = intent?.getStringArrayListExtra(LinkService.EXTRA_REQUEST) ?: return
        intent.removeExtra(LinkService.EXTRA_REQUEST)
        request(names)
    }

    override fun onResume() {
        super.onResume()
        LinkService.start(this)
        LinkService.instance?.onChange = ::render
        // Something may have been granted while we were in Settings.
        LinkService.instance?.sendPermissions()
        render()
        queue.removeFirstOrNull()?.let(::openSetting)
    }

    override fun onPause() {
        LinkService.instance?.onChange = null
        super.onPause()
    }

    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) {
        super.onRequestPermissionsResult(code, permissions, results)
        LinkService.start(this)
        LinkService.instance?.sendPermissions()
        render()
        queue.removeFirstOrNull()?.let(::openSetting)
    }

    private fun request(names: List<String>) {
        val runtime = names.flatMap { Perms.runtime(it).toList() }.filter { !Perms.has(this, it) }
        names.filter { Perms.runtime(it).isEmpty() && !Perms.granted(this, it) }.forEach(queue::addLast)
        if (runtime.isNotEmpty()) {
            requestPermissions(runtime.toTypedArray(), 1)
        } else {
            queue.removeFirstOrNull()?.let(::openSetting)
        }
    }

    private fun openSetting(name: String) {
        val intent = when (name) {
            Perms.NOTIFICATIONS -> Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
            Perms.BATTERY -> Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName"))
            Perms.WALLPAPER -> Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName"))
            else -> return
        }
        try { startActivity(intent) } catch (e: Exception) { }
    }

    private fun render() {
        val service = LinkService.instance
        val bluetooth = Perms.granted(this, Perms.BLUETOOTH)
        state.text = when {
            !bluetooth -> "Needs permission to use Bluetooth"
            service?.connected == true -> "Connected to ${service.peer.ifEmpty { "your computer" }}"
            else -> "Waiting for your computer"
        }
        state.setTextColor(if (service?.connected == true) green else secondary)
        hint.text = if (service?.connected == true) {
            "You can close this. To send text to the computer, share it to Omalink or tap “Send clipboard” in the notification."
        } else {
            "Pair this phone with your computer from Omalink on the desktop: open Bluetooth settings here so the phone can be found, then press Scan there."
        }

        list.removeAllViews()
        val missing = Perms.all.filter { !Perms.granted(this, it) }
        if (missing.size > 1) {
            list.addView(text("Allow everything", 15f, Color.BLACK, true).apply {
                gravity = Gravity.CENTER
                background = rounded(accent, 12)
                setPadding(0, dp(13), 0, dp(13))
                setOnClickListener { request(missing) }
            }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(12) })
        }
        for (name in Perms.all) {
            val (title, detail) = Perms.labels[name] ?: continue
            val ok = name !in missing
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                background = rounded(surface, 12)
                setPadding(dp(16), dp(13), dp(14), dp(13))
            }
            val labels = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            labels.addView(text(title, 15f, foreground, true))
            labels.addView(text(detail, 13f, secondary))
            row.addView(labels, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            row.addView(text(if (ok) "Allowed" else "Allow", 13.5f, if (ok) green else accent, true).apply {
                setPadding(dp(12), dp(8), dp(4), dp(8))
            })
            if (!ok) row.setOnClickListener { request(listOf(name)) }
            row.contentDescription = "$title, ${if (ok) "allowed" else "not allowed"}"
            list.addView(row, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(8) })
        }
        list.visibility = View.VISIBLE
    }
}
