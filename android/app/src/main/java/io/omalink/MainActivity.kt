package io.omalink

import android.app.Activity
import android.app.DownloadManager
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.text.TextUtils
import android.text.format.DateUtils
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

/**
 * The whole interface: the computer and whether it is connected, sending it
 * files or the clipboard, what has been transferred, and what the user has
 * allowed. Everything else happens on the computer.
 */
class MainActivity : Activity() {
    private val backdrop = Color.parseColor("#16171B")
    private val surface = Color.parseColor("#1F2126")
    private val ink = Color.parseColor("#E6E8EC")
    private val secondary = Color.parseColor("#A4A9B3")
    private val accent = Color.parseColor("#9ECE6A")
    private val green = Color.parseColor("#7FC27A")

    private companion object {
        const val PICK = 7
    }

    /** Everything under the title; rebuilt whenever something changes. */
    private lateinit var list: LinearLayout
    private var showPermissions = false

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
        column.addView(text("Omalink", 28f, ink, true))
        column.addView(text("FOR OMARCHY", 12f, accent, true).apply {
            typeface = Typeface.MONOSPACE
            letterSpacing = 0.18f
            setPadding(0, dp(2), 0, dp(6))
        })
        list = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(list, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))

        setContentView(ScrollView(this).apply {
            setBackgroundColor(backdrop)
            isFillViewport = true
            fitsSystemWindows = true
            addView(column)
        })
        accept(intent)
        // First launch: the guided setup does the asking, one thing at a time.
        if (saved == null && SetupActivity.needed(this)) SetupActivity.start(this)
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
        Net.onChange = ::render
        // Something may have been granted while we were in Settings.
        LinkService.instance?.sendPermissions()
        render()
        queue.removeFirstOrNull()?.let(::openSetting)
    }

    override fun onPause() {
        LinkService.instance?.onChange = null
        Net.onChange = null
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
            Perms.CONTROL -> Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
            else -> return
        }
        try { startActivity(intent) } catch (e: Exception) { }
    }

    private fun icon(resource: Int, size: Int, color: Int) = ImageView(this).apply {
        setImageResource(resource)
        setColorFilter(color)
        layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
    }

    private fun spaced(view: View, top: Int) =
        list.addView(view, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(top) })

    /** One of the two big buttons under the computer. */
    private fun tile(resource: Int, label: String, enabled: Boolean, action: () -> Unit) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER
        background = rounded(surface, 14)
        setPadding(dp(8), dp(18), dp(8), dp(16))
        alpha = if (enabled) 1f else 0.4f
        addView(icon(resource, 24, if (enabled) accent else secondary))
        addView(text(label, 13.5f, ink, true).apply { gravity = Gravity.CENTER; setPadding(0, dp(9), 0, 0) })
        contentDescription = label
        if (enabled) setOnClickListener { action() }
    }

    /** A full-width row that leads somewhere. */
    private fun row(resource: Int, title: String, detail: String, tint: Int, action: () -> Unit) = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        background = rounded(surface, 14)
        setPadding(dp(16), dp(15), dp(12), dp(15))
        addView(icon(resource, 22, tint))
        val labels = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(14), 0, 0, 0) }
        labels.addView(text(title, 15f, ink, true))
        if (detail.isNotEmpty()) labels.addView(text(detail, 12.5f, secondary))
        addView(labels, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
        addView(icon(R.drawable.ic_chevron, 18, secondary))
        contentDescription = title
        setOnClickListener { action() }
    }

    private fun start(intent: Intent) = try { startActivity(intent) } catch (e: Exception) { }

    override fun onActivityResult(code: Int, result: Int, data: Intent?) {
        super.onActivityResult(code, result, data)
        if (code != PICK || result != RESULT_OK || data == null) return
        val service = LinkService.instance ?: return
        val clip = data.clipData
        val files = if (clip != null) (0 until clip.itemCount).map { clip.getItemAt(it).uri } else listOfNotNull(data.data)
        val sent = files.count { service.transfers.offer(this, it) }
        if (sent > 0) Toast.makeText(this, if (sent == 1) "Sending to your computer…" else "Sending $sent files to your computer…", Toast.LENGTH_SHORT).show()
    }

    private fun render() {
        val service = LinkService.instance
        val connected = service?.connected == true
        val missing = Perms.all.filter { !Perms.granted(this, it) }
        list.removeAllViews()

        // The computer, and whether it is there.
        val hero = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, dp(18), 0, dp(4))
        }
        hero.addView(LinearLayout(this).apply {
            gravity = Gravity.CENTER
            background = rounded(surface, 28)
            layoutParams = LinearLayout.LayoutParams(dp(112), dp(112))
            addView(icon(R.drawable.ic_computer, 56, if (connected) accent else secondary))
        })
        hero.addView(text(if (connected) service?.peer?.ifEmpty { "Your computer" } ?: "Your computer" else "No computer connected", 20f, ink, true).apply {
            gravity = Gravity.CENTER
            setPadding(0, dp(14), 0, dp(4))
        })
        val status = when {
            connected && service?.bluetooth != true -> if (Net.route == "relay") "Connected over the internet" else "Connected over the network"
            connected -> "Connected"
            Net.paired().isEmpty() && !Perms.granted(this, Perms.BLUETOOTH) -> "Needs permission to use Bluetooth"
            else -> "Waiting for your computer"
        }
        hero.addView(text("●  $status", 14f, if (connected) green else secondary).apply { gravity = Gravity.CENTER })
        spaced(hero, 0)

        val actions = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        actions.addView(tile(R.drawable.ic_send, "Send files", connected) {
            val pick = Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*").putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            try { startActivityForResult(pick, PICK) } catch (e: Exception) { }
        }, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginEnd = dp(6) })
        actions.addView(tile(R.drawable.ic_clipboard, "Send clipboard", connected) {
            start(Intent(this, ShareActivity::class.java).setAction(ShareActivity.SEND_CLIPBOARD))
        }, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginStart = dp(6) })
        spaced(actions, 20)

        if (!connected && Net.paired().isEmpty()) {
            spaced(text(
                "Pair this phone from Omalink on your computer: open Bluetooth settings here so the phone can be found, then press Scan there. Or pair over the network below to reach it from anywhere.",
                13.5f, secondary,
            ).apply { setLineSpacing(0f, 1.2f) }, 16)
        }

        if (missing.isNotEmpty()) {
            spaced(row(R.drawable.ic_shield, "Finish setup", "${missing.size} left · a minute, step by step", accent) {
                SetupActivity.start(this)
            }, 16)
        }

        val paired = Net.paired()
        spaced(row(
            R.drawable.ic_computer,
            if (paired.isEmpty()) "Pair with a computer" else "Computers",
            if (paired.isEmpty()) "Over Wi-Fi or the internet, checked with icons" else paired.joinToString(", ") { it.second.ifEmpty { "Computer" } },
            if (paired.isEmpty()) accent else secondary,
        ) { start(Intent(this, PairActivity::class.java)) }, 16)

        // What has gone back and forth since the app started.
        val recent = service?.transfers?.recent()?.take(5) ?: emptyList()
        if (recent.isNotEmpty()) {
            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                background = rounded(surface, 14)
                setPadding(dp(16), dp(14), dp(16), dp(8))
            }
            card.addView(text("Recent", 15f, ink, true).apply { setPadding(0, 0, 0, dp(6)) })
            for (item in recent) {
                val line = LinearLayout(this).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, dp(9), 0, dp(9))
                }
                line.addView(icon(if (item.incoming) R.drawable.ic_in else R.drawable.ic_out, 20, accent))
                val labels = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(14), 0, 0, 0) }
                labels.addView(text(item.name, 14.5f, ink, true).apply { maxLines = 1; ellipsize = TextUtils.TruncateAt.MIDDLE })
                val time = DateUtils.getRelativeTimeSpanString(item.time, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS)
                labels.addView(text((if (item.incoming) "From your computer" else "Sent to your computer") + "  ·  $time", 12.5f, secondary))
                line.addView(labels, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
                item.uri?.let { uri ->
                    line.setOnClickListener {
                        start(Intent(Intent.ACTION_VIEW).setDataAndType(uri, contentResolver.getType(uri) ?: "*/*").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                    }
                }
                card.addView(line)
            }
            spaced(card, 16)
        }

        spaced(row(R.drawable.ic_folder, "Received files", "Downloads/Omalink", secondary) {
            start(Intent(DownloadManager.ACTION_VIEW_DOWNLOADS))
        }, 16)
        // Open by itself while something still needs allowing.
        val open = showPermissions || missing.isNotEmpty()
        spaced(row(
            R.drawable.ic_shield, "App permissions",
            if (missing.isEmpty()) "Everything is allowed" else "${missing.size} still to allow",
            if (missing.isEmpty()) secondary else accent,
        ) { showPermissions = !showPermissions; render() }, 8)
        if (!open) return

        if (missing.size > 1) {
            spaced(text("Allow everything", 15f, Color.BLACK, true).apply {
                gravity = Gravity.CENTER
                background = rounded(accent, 12)
                setPadding(0, dp(13), 0, dp(13))
                setOnClickListener { request(missing) }
            }, 12)
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
            labels.addView(text(title, 15f, ink, true))
            labels.addView(text(detail, 13f, secondary))
            row.addView(labels, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            row.addView(text(if (ok) "Allowed" else "Allow", 13.5f, if (ok) green else accent, true).apply {
                setPadding(dp(12), dp(8), dp(4), dp(8))
            })
            if (!ok) row.setOnClickListener { request(listOf(name)) }
            row.contentDescription = "$title, ${if (ok) "allowed" else "not allowed"}"
            spaced(row, 8)
        }
        spaced(text(
            "If Android greys out Notification access or Messages for this app, open App info → ⋮ → Allow restricted settings. Android does that for apps installed outside the Play Store.",
            12.5f, secondary,
        ).apply { setLineSpacing(0f, 1.2f) }, 14)
    }
}
