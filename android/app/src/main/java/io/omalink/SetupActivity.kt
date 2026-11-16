package io.omalink

import android.animation.ValueAnimator
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.animation.DecelerateInterpolator
import android.view.animation.OvershootInterpolator
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The guided setup: one thing at a time, each with why it's needed and one
 * button, moving on by itself once it's allowed. Android's "restricted
 * setting" wall, which every sideloaded app hits, gets its own walkthrough.
 */
class SetupActivity : Activity() {
    companion object {
        private const val PREFS = "omalink"
        private const val DONE = "setup_done"

        fun needed(context: Context) = !context.getSharedPreferences(PREFS, MODE_PRIVATE).getBoolean(DONE, false)
        fun start(context: Context) = context.startActivity(Intent(context, SetupActivity::class.java))
    }

    private val backdrop = Color.parseColor("#16171B")
    private val surface = Color.parseColor("#1F2126")
    private val ink = Color.parseColor("#E6E8EC")
    private val secondary = Color.parseColor("#A4A9B3")
    private val accent = Color.parseColor("#9ECE6A")

    /** One step: what it's for, and how to tell it's done. */
    private class Step(
        val key: String,
        val icon: Int,
        val title: String,
        val why: String,
        val action: String,
        val restricted: Boolean = false,
    )

    private val steps = ArrayList<Step>()
    private var index = 0
    private lateinit var stage: FrameLayout
    private lateinit var bar: View
    private lateinit var barTrack: FrameLayout
    private var page: View? = null
    private var burst: BurstView? = null
    private var advancing = false

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        steps.add(Step("welcome", R.drawable.ic_computer, "Your phone, on your desktop", "Messages, calls, notifications, files and your screen, on Omarchy. A minute of setup and you're in.", "Let's go"))
        val wanted = listOf(
            Step(Perms.POST_NOTIFICATIONS, R.drawable.ic_link, "Stay connected", "A quiet notification keeps Omalink running, so your computer can always reach the phone.", "Allow"),
            Step(Perms.BLUETOOTH, R.drawable.ic_link, "Nearby devices", "To talk to your computer over Bluetooth, and take calls on it.", "Allow"),
            Step(Perms.SMS, R.drawable.ic_send, "Messages", "Read and send your texts from the computer.", "Allow"),
            Step(Perms.CONTACTS, R.drawable.ic_shield, "Contacts", "Names and faces instead of numbers.", "Allow"),
            Step(Perms.CALL_LOG, R.drawable.ic_shield, "Calls", "Recent calls, and placing and answering them.", "Allow"),
            Step(Perms.PHONE, R.drawable.ic_shield, "Phone", "Ring, answer and hang up from the computer.", "Allow"),
            Step(Perms.PHOTOS, R.drawable.ic_folder, "Photos and videos", "Your gallery on the computer: browse, download, tidy up.", "Allow"),
            Step(Perms.NOTIFICATIONS, R.drawable.ic_link, "Notification access", "See and answer this phone's notifications on the computer.", "Open settings", restricted = true),
            Step(Perms.BATTERY, R.drawable.ic_link, "Run in background", "Stops Android from putting Omalink to sleep while the screen is off.", "Allow"),
            Step(Perms.CONTROL, R.drawable.ic_computer, "Control from computer", "Use your mouse and keyboard on this phone while you share its screen. Only during a session you approve.", "Open settings", restricted = true),
        )
        steps.addAll(wanted.filter { !Perms.granted(this, it.key) })
        if (Net.paired().isEmpty()) {
            steps.add(Step("pair", R.drawable.ic_computer, "Pair with your computer", "On the computer, open Omalink and press Pair a phone. Then pick it here and tap the matching icon.", "Pair now"))
        }
        steps.add(Step("done", R.drawable.ic_computer, "You're all set", "Open Omalink on your computer. Your phone is waiting there.", "Finish"))

        val root = FrameLayout(this).apply { setBackgroundColor(backdrop); fitsSystemWindows = true }
        barTrack = FrameLayout(this).apply {
            background = GradientDrawable().apply { setColor(surface); cornerRadius = dp(3).toFloat() }
        }
        bar = View(this).apply { background = GradientDrawable().apply { setColor(accent); cornerRadius = dp(3).toFloat() } }
        barTrack.addView(bar, FrameLayout.LayoutParams(0, MATCH_PARENT))
        root.addView(barTrack, FrameLayout.LayoutParams(MATCH_PARENT, dp(6)).apply { setMargins(dp(28), dp(24), dp(28), 0) })
        stage = FrameLayout(this)
        root.addView(stage, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT).apply { topMargin = dp(40) })
        setContentView(root)
        LinkService.start(this)
        show(0, forward = true)
    }

    override fun onResume() {
        super.onResume()
        // Back from Settings, or a permission prompt: was it allowed?
        val step = steps.getOrNull(index) ?: return
        if (step.key == "pair" && Net.paired().isNotEmpty()) celebrate()
        else if (step.key !in setOf("welcome", "pair", "done") && Perms.granted(this, step.key)) celebrate()
    }

    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) {
        super.onRequestPermissionsResult(code, permissions, results)
        LinkService.start(this)
        LinkService.instance?.sendPermissions()
        onResume()
    }

    private fun progress() {
        val fraction = index.toFloat() / (steps.size - 1).coerceAtLeast(1)
        barTrack.post {
            val target = (barTrack.width * fraction).toInt().coerceAtLeast(dp(6))
            ValueAnimator.ofInt(bar.layoutParams.width, target).apply {
                duration = 420
                interpolator = DecelerateInterpolator()
                addUpdateListener { bar.layoutParams = bar.layoutParams.apply { width = it.animatedValue as Int } }
                start()
            }
        }
    }

    private fun text(value: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        text = value
        textSize = size
        setTextColor(color)
        gravity = Gravity.CENTER
        setLineSpacing(0f, 1.2f)
        if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    }

    private fun button(label: String, primary: Boolean, action: () -> Unit) = text(label, 16f, if (primary) Color.BLACK else secondary, true).apply {
        background = GradientDrawable().apply { setColor(if (primary) accent else Color.TRANSPARENT); cornerRadius = dp(16).toFloat() }
        setPadding(0, dp(16), 0, dp(16))
        setOnClickListener { action() }
    }

    /** Lays out step `i`, sliding it in from the side it's coming from. */
    private fun show(i: Int, forward: Boolean) {
        index = i
        advancing = false
        progress()
        val step = steps[i]
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(28), dp(36), dp(28), dp(28))
        }

        // The icon, breathing inside a ring.
        val halo = FrameLayout(this)
        val glow = View(this).apply { background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(Color.argb(40, 158, 206, 106)) } }
        halo.addView(glow, FrameLayout.LayoutParams(dp(176), dp(176), Gravity.CENTER))
        val disc = FrameLayout(this).apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(surface) }
            addView(ImageView(context).apply { setImageResource(step.icon); setColorFilter(accent) }, FrameLayout.LayoutParams(dp(58), dp(58), Gravity.CENTER))
        }
        halo.addView(disc, FrameLayout.LayoutParams(dp(128), dp(128), Gravity.CENTER))
        val celebration = BurstView(this, accent)
        halo.addView(celebration, FrameLayout.LayoutParams(dp(240), dp(240), Gravity.CENTER))
        burst = celebration
        ValueAnimator.ofFloat(0.92f, 1.08f).apply {
            duration = 1600
            repeatMode = ValueAnimator.REVERSE
            repeatCount = ValueAnimator.INFINITE
            addUpdateListener { glow.scaleX = it.animatedValue as Float; glow.scaleY = it.animatedValue as Float }
            start()
        }
        column.addView(halo, LinearLayout.LayoutParams(dp(240), dp(240)))

        column.addView(text(step.title, 26f, ink, true), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(8) })
        column.addView(text(step.why, 15.5f, secondary), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(12) })
        column.addView(View(this), LinearLayout.LayoutParams(1, 0, 1f))

        // The wall Android puts in front of sideloaded apps, and the way over it.
        if (step.restricted) {
            val help = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                background = GradientDrawable().apply { setColor(surface); cornerRadius = dp(16).toFloat() }
                setPadding(dp(18), dp(16), dp(18), dp(16))
            }
            help.addView(text("Greyed out, or “App was denied access”?", 14.5f, ink, true).apply { gravity = Gravity.START })
            help.addView(text("Android does that for apps from outside the Play Store. Open App info, tap ⋮ at the top right, then Allow restricted settings. Come back and try again.", 13.5f, secondary).apply { gravity = Gravity.START; setPadding(0, dp(6), 0, dp(10)) })
            help.addView(text("Open App info", 14.5f, accent, true).apply {
                gravity = Gravity.START
                setOnClickListener { open(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName"))) }
            })
            column.addView(help, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(16) })
        }

        column.addView(button(step.action, true) { act(step) }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        if (step.key != "welcome" && step.key != "done") {
            column.addView(button("Skip for now", false) { next() }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(6) })
        }

        // Out with the old, in with the new.
        val width = resources.displayMetrics.widthPixels.toFloat()
        page?.let { old ->
            old.animate().translationX(if (forward) -width * 0.35f else width * 0.35f).alpha(0f).setDuration(260)
                .withEndAction { stage.removeView(old) }.start()
        }
        column.translationX = if (forward) width * 0.35f else -width * 0.35f
        column.alpha = 0f
        stage.addView(column, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        column.animate().translationX(0f).alpha(1f).setDuration(380).setInterpolator(DecelerateInterpolator(1.6f)).start()
        disc.scaleX = 0.6f; disc.scaleY = 0.6f
        disc.animate().scaleX(1f).scaleY(1f).setStartDelay(120).setDuration(480).setInterpolator(OvershootInterpolator(2.2f)).start()
        page = column

        if (step.key == "done") {
            getSharedPreferences(PREFS, MODE_PRIVATE).edit().putBoolean(DONE, true).apply()
            column.postDelayed({ celebration.play() }, 450)
        }
    }

    private fun act(step: Step) {
        when (step.key) {
            "welcome" -> next()
            "done" -> { startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP)); finish() }
            "pair" -> open(Intent(this, PairActivity::class.java))
            Perms.NOTIFICATIONS -> open(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
            Perms.BATTERY -> open(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName")))
            Perms.CONTROL -> open(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
            Perms.WALLPAPER -> if (Build.VERSION.SDK_INT >= 30) open(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName")))
            else -> {
                val runtime = Perms.runtime(step.key).filter { !Perms.has(this, it) }
                if (runtime.isEmpty()) celebrate() else requestPermissions(runtime.toTypedArray(), 1)
            }
        }
    }

    private fun open(intent: Intent) = try { startActivity(intent) } catch (e: Exception) { }

    /** It's allowed: a burst, then the next step. */
    private fun celebrate() {
        if (advancing) return
        advancing = true
        burst?.play()
        page?.postDelayed({ next() }, 750)
    }

    private fun next() {
        if (index + 1 < steps.size) show(index + 1, forward = true)
    }

    @Deprecated("Back goes back a step.")
    override fun onBackPressed() {
        if (index > 0) show(index - 1, forward = false) else super.onBackPressed()
    }
}
