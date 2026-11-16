package io.omalink

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.app.Activity
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.util.Size
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.ViewOutlineProvider
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.view.animation.LinearInterpolator
import android.view.animation.OvershootInterpolator
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * Omadrop: what sharing to the computer looks like. A sheet slides up with
 * the computer in the middle of a radar; the files fan out above it, fly
 * into it as they go, a ring fills while they travel, and a burst and a
 * check say they've landed.
 */
class Omadrop(private val activity: Activity, private val onClose: () -> Unit) : Transfers.Watcher {
    private val accent = Color.parseColor("#9ECE6A")
    private val ink = Color.parseColor("#E6E8EC")
    private val secondary = Color.parseColor("#A4A9B3")
    private val red = Color.parseColor("#E5676B")
    private val surface = Color.parseColor("#1F2126")

    private fun dp(v: Float) = v * activity.resources.displayMetrics.density
    private fun dpi(v: Int) = dp(v.toFloat()).toInt()

    /** A file on its way: what's been sent of how much. */
    private class Item(val uri: Uri, val name: String, val size: Long) {
        var sent = 0L
        var done = false
        var ok = false
    }

    private val items = ArrayList<Item>()
    private lateinit var dim: View
    private lateinit var sheet: LinearLayout
    private lateinit var stage: FrameLayout
    private lateinit var ring: RingView
    private lateinit var radar: RadarView
    private lateinit var burst: BurstView
    private lateinit var core: FrameLayout
    private lateinit var status: TextView
    private lateinit var button: TextView
    private val thumbs = ArrayList<View>()
    private var flown = false
    private var ended = false

    fun show(uris: List<Uri>, computer: String, connected: Boolean) {
        for (uri in uris) items.add(describe(uri))
        build(computer)
        enter()
        if (!connected) return fail("Your computer isn't connected")
        status.text = if (items.size == 1) "Sending ${items[0].name}" else "Sending ${items.size} files"
        // A beat to see what's going, then off they go.
        stage.postDelayed({ fly() }, 650)
    }

    /** Text sent to the computer's clipboard gets the short version: straight to the check. */
    fun showText(computer: String, connected: Boolean) {
        build(computer)
        enter()
        if (!connected) return fail("Your computer isn't connected")
        status.text = "Copied to $computer"
        stage.postDelayed({ ring.set(1f); succeed() }, 450)
    }

    private fun describe(uri: Uri): Item {
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "file"
        var size = -1L
        try {
            activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    c.getString(0)?.let { name = it }
                    if (!c.isNull(1)) size = c.getLong(1)
                }
            }
        } catch (e: Exception) { }
        return Item(uri, name, size)
    }

    private fun build(computer: String) {
        val root = FrameLayout(activity)
        dim = View(activity).apply { setBackgroundColor(Color.argb(150, 0, 0, 0)); alpha = 0f; setOnClickListener { if (ended) close() } }
        root.addView(dim, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))

        sheet = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            background = GradientDrawable().apply {
                setColor(surface)
                val r = dp(28f)
                cornerRadii = floatArrayOf(r, r, r, r, 0f, 0f, 0f, 0f)
            }
            setPadding(dpi(24), dpi(12), dpi(24), dpi(28))
            isClickable = true
        }
        sheet.addView(View(activity).apply {
            background = GradientDrawable().apply { setColor(Color.argb(60, 255, 255, 255)); cornerRadius = dp(3f) }
        }, LinearLayout.LayoutParams(dpi(40), dpi(5)).apply { bottomMargin = dpi(8) })

        stage = FrameLayout(activity).apply { clipChildren = false; clipToPadding = false }
        radar = RadarView(activity, accent)
        stage.addView(radar, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        ring = RingView(activity, accent)
        stage.addView(ring, FrameLayout.LayoutParams(dpi(132), dpi(132), Gravity.CENTER).apply { topMargin = dpi(40) })
        core = FrameLayout(activity).apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(Color.parseColor("#2A2D34")) }
            addView(ImageView(activity).apply {
                setImageResource(R.drawable.ic_computer)
                setColorFilter(accent)
            }, FrameLayout.LayoutParams(dpi(46), dpi(46), Gravity.CENTER))
        }
        stage.addView(core, FrameLayout.LayoutParams(dpi(108), dpi(108), Gravity.CENTER).apply { topMargin = dpi(40) })
        burst = BurstView(activity, accent)
        stage.addView(burst, FrameLayout.LayoutParams(dpi(240), dpi(240), Gravity.CENTER).apply { topMargin = dpi(40) })

        // The files, fanned out above the computer.
        val shown = items.take(3)
        for ((i, item) in shown.withIndex()) {
            val tile = thumb(item)
            val offset = (i - (shown.size - 1) / 2f)
            tile.rotation = offset * 9f
            tile.translationX = dp(offset * 34f)
            tile.scaleX = 0f; tile.scaleY = 0f
            stage.addView(tile, FrameLayout.LayoutParams(dpi(76), dpi(76), Gravity.CENTER_HORIZONTAL or Gravity.TOP).apply { topMargin = dpi(4) })
            thumbs.add(tile)
        }
        sheet.addView(stage, LinearLayout.LayoutParams(MATCH_PARENT, dpi(300)))

        sheet.addView(TextView(activity).apply {
            text = computer
            textSize = 21f
            setTextColor(ink)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            gravity = Gravity.CENTER
        }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        status = TextView(activity).apply {
            textSize = 14.5f
            setTextColor(secondary)
            gravity = Gravity.CENTER
            maxLines = 2
        }
        sheet.addView(status, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dpi(6) })
        button = TextView(activity).apply {
            text = "Hide"
            textSize = 15f
            setTextColor(ink)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            gravity = Gravity.CENTER
            background = GradientDrawable().apply { setColor(Color.parseColor("#2A2D34")); cornerRadius = dp(14f) }
            setPadding(0, dpi(14), 0, dpi(14))
            setOnClickListener { close() }
        }
        sheet.addView(button, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dpi(22) })

        root.addView(sheet, FrameLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT, Gravity.BOTTOM))
        activity.setContentView(root)
    }

    private fun thumb(item: Item): View {
        val frame = FrameLayout(activity).apply {
            background = GradientDrawable().apply { setColor(Color.parseColor("#2A2D34")); cornerRadius = dp(16f) }
            elevation = dp(8f)
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(view: View, outline: Outline) = outline.setRoundRect(0, 0, view.width, view.height, dp(16f))
            }
            clipToOutline = true
        }
        val label = TextView(activity).apply {
            text = item.name.substringAfterLast('.', "").uppercase().take(4).ifEmpty { "FILE" }
            setTextColor(accent)
            textSize = 13f
            typeface = Typeface.create("sans-serif-medium", Typeface.BOLD)
            gravity = Gravity.CENTER
        }
        frame.addView(label, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        val picture = ImageView(activity).apply { scaleType = ImageView.ScaleType.CENTER_CROP }
        frame.addView(picture, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        // Pictures and videos show themselves; loading happens off the main thread.
        Thread {
            val bitmap = try { activity.contentResolver.loadThumbnail(item.uri, Size(240, 240), null) } catch (e: Exception) { null }
            if (bitmap != null) activity.runOnUiThread { picture.setImageBitmap(bitmap); label.visibility = View.GONE }
        }.start()
        return frame
    }

    private fun enter() {
        sheet.post {
            sheet.translationY = sheet.height.toFloat()
            sheet.animate().translationY(0f).setDuration(460).setInterpolator(OvershootInterpolator(0.9f)).start()
            dim.animate().alpha(1f).setDuration(260).start()
            for ((i, tile) in thumbs.withIndex()) {
                tile.animate().scaleX(1f).scaleY(1f).setStartDelay(260L + i * 90).setDuration(360).setInterpolator(OvershootInterpolator(2f)).start()
            }
            radar.start()
        }
    }

    /** The files dive into the computer. */
    private fun fly() {
        if (flown || ended) return
        flown = true
        val drop = dp(150f)
        for ((i, tile) in thumbs.withIndex()) {
            tile.animate()
                .translationY(drop).translationX(0f).rotation(0f)
                .scaleX(0.2f).scaleY(0.2f).alpha(0f)
                .setStartDelay(i * 110L).setDuration(560)
                .setInterpolator(AccelerateInterpolator(1.4f))
                .start()
        }
        // The computer gulps each one.
        core.animate().scaleX(1.08f).scaleY(1.08f).setStartDelay(520).setDuration(140).withEndAction {
            core.animate().scaleX(1f).scaleY(1f).setDuration(220).setInterpolator(OvershootInterpolator(3f)).start()
        }.start()
    }

    override fun progress(name: String, sent: Long, total: Long) {
        val item = items.firstOrNull { it.name == name && !it.done } ?: return
        item.sent = sent
        if (!flown) fly()
        update()
    }

    override fun finished(name: String, ok: Boolean, reason: String) {
        val item = items.firstOrNull { it.name == name && !it.done } ?: return
        item.done = true
        item.ok = ok
        if (ok && item.size > 0) item.sent = item.size
        if (!ok) return fail(reason.ifEmpty { "Couldn't send ${item.name}" })
        update()
        if (items.all { it.done }) succeed()
    }

    private fun update() {
        val known = items.filter { it.size > 0 }
        val fraction = if (known.isEmpty()) items.count { it.done }.toFloat() / items.size.coerceAtLeast(1)
                       else known.sumOf { it.sent }.toFloat() / known.sumOf { it.size }.coerceAtLeast(1)
        ring.set(fraction)
        val left = items.count { !it.done }
        if (left > 0) status.text = if (items.size == 1) "Sending ${items[0].name}" else "Sending ${items.size - left + 1} of ${items.size}"
    }

    private fun succeed() {
        if (ended) return
        ended = true
        fly()
        radar.stop()
        ring.set(1f)
        (core.background as GradientDrawable).setColor(accent)
        (core.getChildAt(0) as ImageView).animate().alpha(0f).setDuration(150).start()
        burst.play()
        if (Build.VERSION.SDK_INT >= 30) stage.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
        else stage.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        if (status.text.isNullOrEmpty() || status.text.startsWith("Sending")) {
            status.text = if (items.size <= 1) "Sent" else "All ${items.size} sent"
        }
        status.setTextColor(accent)
        button.text = "Done"
        stage.postDelayed({ close() }, 1600)
    }

    private fun fail(reason: String) {
        if (ended) return
        ended = true
        radar.stop()
        status.text = reason
        status.setTextColor(red)
        ring.tint(red)
        button.text = "Close"
        // A no-shake.
        ObjectAnimator.ofFloat(stage, "translationX", 0f, -22f, 20f, -14f, 10f, -5f, 0f).setDuration(520).start()
        if (Build.VERSION.SDK_INT >= 30) stage.performHapticFeedback(HapticFeedbackConstants.REJECT)
    }

    private var closing = false
    fun close() {
        if (closing) return
        closing = true
        dim.animate().alpha(0f).setDuration(220).start()
        sheet.animate().translationY(sheet.height.toFloat()).setDuration(260).setInterpolator(AccelerateInterpolator())
            .withEndAction { onClose() }.start()
    }
}

/** Rings that ripple out from the middle while the computer is listening. */
class RadarView(context: Context, private val color: Int) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private var phase = 0f
    private var fade = 1f
    private val animator = ValueAnimator.ofFloat(0f, 1f).apply {
        duration = 2400
        repeatCount = ValueAnimator.INFINITE
        interpolator = LinearInterpolator()
        addUpdateListener { phase = it.animatedValue as Float; invalidate() }
    }

    fun start() = animator.start()
    fun stop() {
        ValueAnimator.ofFloat(1f, 0f).apply {
            duration = 400
            addUpdateListener { fade = it.animatedValue as Float; invalidate() }
            addListener(object : AnimatorListenerAdapter() { override fun onAnimationEnd(a: Animator) = animator.cancel() })
        }.start()
    }

    override fun onDetachedFromWindow() { animator.cancel(); super.onDetachedFromWindow() }

    override fun onDraw(canvas: Canvas) {
        val cx = width / 2f
        val cy = height / 2f + 40 * resources.displayMetrics.density
        val base = 54 * resources.displayMetrics.density
        val reach = min(width, height) * 0.62f
        for (i in 0 until 3) {
            val t = (phase + i / 3f) % 1f
            paint.strokeWidth = (1 - t) * 3 * resources.displayMetrics.density + 1
            paint.color = Color.argb(((1 - t) * 160 * fade).toInt(), Color.red(color), Color.green(color), Color.blue(color))
            canvas.drawCircle(cx, cy, base + t * (reach - base), paint)
        }
    }
}

/** How far along the files are, as a ring around the computer. */
class RingView(context: Context, private var color: Int) : View(context) {
    private val track = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; color = Color.argb(40, 255, 255, 255) }
    private val arc = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private var shown = 0f
    private var animator: ValueAnimator? = null

    fun set(fraction: Float) {
        animator?.cancel()
        animator = ValueAnimator.ofFloat(shown, fraction.coerceIn(0f, 1f)).apply {
            duration = 260
            interpolator = DecelerateInterpolator()
            addUpdateListener { shown = it.animatedValue as Float; invalidate() }
            start()
        }
    }

    fun tint(newColor: Int) { color = newColor; invalidate() }

    override fun onDraw(canvas: Canvas) {
        val stroke = 5 * resources.displayMetrics.density
        track.strokeWidth = stroke
        arc.strokeWidth = stroke
        arc.color = color
        val box = RectF(stroke, stroke, width - stroke, height - stroke)
        canvas.drawOval(box, track)
        if (shown > 0f) canvas.drawArc(box, -90f, 360f * shown, false, arc)
    }
}

/** Confetti out of the middle, and a check drawn in. */
class BurstView(context: Context, private val color: Int) : View(context) {
    private val dots = Paint(Paint.ANTI_ALIAS_FLAG)
    private val tick = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND; color = Color.parseColor("#10140F")
    }
    private var t = -1f
    private val palette = intArrayOf(color, Color.parseColor("#E6C36A"), Color.parseColor("#7FC27A"), Color.parseColor("#88C0D0"), Color.WHITE)

    fun play() {
        ValueAnimator.ofFloat(0f, 1f).apply {
            duration = 900
            interpolator = DecelerateInterpolator(1.6f)
            addUpdateListener { t = it.animatedValue as Float; invalidate() }
            start()
        }
    }

    override fun onDraw(canvas: Canvas) {
        if (t < 0f) return
        val d = resources.displayMetrics.density
        val cx = width / 2f
        val cy = height / 2f
        for (i in 0 until 18) {
            val angle = Math.toRadians(i * 20.0 + (i % 3) * 7)
            val reach = (54 + t * (60 + (i % 4) * 14)) * d
            dots.color = palette[i % palette.size]
            dots.alpha = ((1 - t) * 255).toInt()
            canvas.drawCircle(cx + cos(angle).toFloat() * reach, cy + sin(angle).toFloat() * reach, (4 - t * 2) * d, dots)
        }
        // The check, drawn stroke by stroke in the first half.
        val drawn = min(1f, t * 2f)
        tick.strokeWidth = 6 * d
        val path = Path()
        val a = floatArrayOf(cx - 18 * d, cy + 1 * d)
        val b = floatArrayOf(cx - 5 * d, cy + 14 * d)
        val c = floatArrayOf(cx + 20 * d, cy - 13 * d)
        path.moveTo(a[0], a[1])
        if (drawn < 0.4f) {
            val k = drawn / 0.4f
            path.lineTo(a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k)
        } else {
            val k = (drawn - 0.4f) / 0.6f
            path.lineTo(b[0], b[1])
            path.lineTo(b[0] + (c[0] - b[0]) * k, b[1] + (c[1] - b[1]) * k)
        }
        canvas.drawPath(path, tick)
    }
}
