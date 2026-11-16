package io.omalink

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import org.json.JSONObject

/**
 * Lets the computer drive the phone during a screen-sharing session: its
 * pointer becomes a finger, its wheel a swipe, its keyboard typing.
 *
 * Android only lets an accessibility service do this, and only once the
 * person has switched it on in Settings. Even then it does nothing outside a
 * session that passed the icon check, which is when [MirrorService.active].
 */
class ControlService : AccessibilityService() {
    companion object {
        @Volatile var instance: ControlService? = null

        private val INPUTS = setOf("input", "scroll", "global_action", "type_text", "key")

        /**
         * Takes an input message from the computer; false if it isn't one.
         * Input outside a verified session, or with the service switched
         * off, is dropped.
         */
        fun handle(type: String, d: JSONObject): Boolean {
            if (type !in INPUTS) return false
            val service = instance ?: return true
            if (!MirrorService.active) return true
            service.main.post {
                when (type) {
                    "input" -> service.pointer(d.getString("kind"), d.getDouble("x").toFloat(), d.getDouble("y").toFloat())
                    "scroll" -> service.scroll(d.getDouble("x").toFloat(), d.getDouble("y").toFloat(), d.optDouble("dx", 0.0).toFloat(), d.optDouble("dy", 0.0).toFloat())
                    "global_action" -> service.global(d.getString("action"))
                    "type_text" -> service.type(d.getString("text"))
                    "key" -> service.key(d.getString("key"))
                }
            }
            return true
        }
    }

    private val main = android.os.Handler(android.os.Looper.getMainLooper())

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
    }

    override fun onDestroy() {
        instance = null
        super.onDestroy()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}
    override fun onInterrupt() {}

    /** The screen in pixels, for turning 0–1 positions into touches. */
    private fun screen(): Pair<Float, Float> {
        val (w, h) = screenSize(this)
        return w.toFloat() to h.toFloat()
    }

    private fun at(x: Float, y: Float): Pair<Float, Float> {
        val (w, h) = screen()
        return (x.coerceIn(0f, 1f) * (w - 1)) to (y.coerceIn(0f, 1f) * (h - 1))
    }

    // A finger that follows the pointer: each move continues the stroke
    // the press began, so drags and long presses act as they happen.
    private var stroke: GestureDescription.StrokeDescription? = null
    private var last = 0f to 0f
    private var lastTime = 0L
    private var downAt = 0f to 0f
    private var downTime = 0L

    private fun pointer(kind: String, x: Float, y: Float) {
        val point = at(x, y)
        val now = SystemClock.uptimeMillis()
        when (kind) {
            "down" -> {
                val path = Path().apply { moveTo(point.first, point.second) }
                stroke = GestureDescription.StrokeDescription(path, 0, 1, true).also(::dispatch)
                downAt = point
                downTime = now
            }
            "move" -> {
                val previous = stroke ?: return
                // Many moves a frame would only queue up behind each other.
                if (now - lastTime < 16) return
                val path = Path().apply { moveTo(last.first, last.second); lineTo(point.first, point.second) }
                stroke = previous.continueStroke(path, 0, (now - lastTime).coerceIn(1, 100), true).also(::dispatch)
            }
            "up" -> {
                val previous = stroke ?: return
                val path = Path().apply { moveTo(last.first, last.second); lineTo(point.first, point.second) }
                previous.continueStroke(path, 0, (now - lastTime).coerceIn(1, 100), false).also(::dispatch)
                stroke = null
            }
        }
        last = point
        lastTime = now
    }

    private fun dispatch(stroke: GestureDescription.StrokeDescription) {
        dispatchGesture(GestureDescription.Builder().addStroke(stroke).build(), object : GestureResultCallback() {
            override fun onCancelled(gesture: GestureDescription?) {
                // Android dropped the held finger, usually because a move came
                // too late; the rest of this drag is lost, the next press
                // starts afresh.
                if (this@ControlService.stroke === stroke) this@ControlService.stroke = null
            }
        }, null)
    }

    /** A wheel notch scrolls about a tenth of the screen. */
    private fun scroll(x: Float, y: Float, dx: Float, dy: Float) {
        val (w, h) = screen()
        val (sx, sy) = at(x, y)
        val ex = (sx + dx * w * 0.1f).coerceIn(1f, w - 2)
        val ey = (sy + dy * h * 0.1f).coerceIn(1f, h - 2)
        val path = Path().apply { moveTo(sx, sy); lineTo(ex, ey) }
        dispatchGesture(GestureDescription.Builder().addStroke(GestureDescription.StrokeDescription(path, 0, 120)).build(), null, null)
    }

    private fun global(action: String) {
        val code = when (action) {
            "back" -> GLOBAL_ACTION_BACK
            "home" -> GLOBAL_ACTION_HOME
            "recents" -> GLOBAL_ACTION_RECENTS
            "notifications" -> GLOBAL_ACTION_NOTIFICATIONS
            "lock" -> if (Build.VERSION.SDK_INT >= 28) GLOBAL_ACTION_LOCK_SCREEN else return
            else -> return
        }
        performGlobalAction(code)
    }

    private fun focused(): AccessibilityNodeInfo? = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)

    /** Puts `text` at the cursor of the focused field, replacing any selection. */
    private fun type(text: String) {
        val field = focused() ?: return
        val current = field.text?.toString() ?: ""
        // A hint shows as the text of an empty field.
        val existing = if (field.isShowingHintText) "" else current
        val start = field.textSelectionStart.takeIf { it >= 0 } ?: existing.length
        val end = field.textSelectionEnd.takeIf { it >= 0 } ?: start
        val (from, to) = minOf(start, end).coerceAtMost(existing.length) to maxOf(start, end).coerceAtMost(existing.length)
        setText(field, existing.substring(0, from) + text + existing.substring(to), from + text.length)
    }

    private fun key(key: String) {
        val field = focused()
        when (key) {
            "enter" -> {
                if (field != null && Build.VERSION.SDK_INT >= 30) {
                    field.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)
                } else {
                    type("\n")
                }
            }
            "backspace", "delete" -> {
                field ?: return
                val text = if (field.isShowingHintText) "" else field.text?.toString() ?: ""
                val start = field.textSelectionStart.takeIf { it >= 0 } ?: text.length
                val end = field.textSelectionEnd.takeIf { it >= 0 } ?: start
                val (from, to) = minOf(start, end).coerceAtMost(text.length) to maxOf(start, end).coerceAtMost(text.length)
                when {
                    from != to -> setText(field, text.removeRange(from, to), from)
                    key == "backspace" && from > 0 -> {
                        // A whole character, even one made of two UTF-16 units.
                        val cut = text.offsetByCodePoints(from, -1)
                        setText(field, text.removeRange(cut, from), cut)
                    }
                    key == "delete" && from < text.length -> setText(field, text.removeRange(from, text.offsetByCodePoints(from, 1)), from)
                }
            }
            "left", "right" -> {
                field ?: return
                val text = field.text?.toString() ?: ""
                val at = field.textSelectionEnd.takeIf { it >= 0 } ?: text.length
                val next = if (key == "left") (if (at > 0) text.offsetByCodePoints(at, -1) else 0) else (if (at < text.length) text.offsetByCodePoints(at, 1) else at)
                select(field, next)
            }
            "back" -> global("back")
            "home" -> global("home")
        }
    }

    private fun setText(field: AccessibilityNodeInfo, text: String, cursor: Int) {
        field.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text)
        })
        select(field, cursor)
    }

    private fun select(field: AccessibilityNodeInfo, cursor: Int) {
        field.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, Bundle().apply {
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, cursor)
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, cursor)
        })
    }
}
