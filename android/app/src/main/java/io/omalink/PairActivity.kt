package io.omalink

import android.app.Activity
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * Pairing with a computer over the network, and the icon check that guards
 * pairing and every screen-sharing session: the computer shows one icon, this
 * shows three, and only the matching one goes through.
 */
class PairActivity : Activity() {
    companion object {
        const val EXTRA_VERIFY = "verify"
        @Volatile var visible = false
        @Volatile var current: PairActivity? = null
    }

    private val backdrop = Color.parseColor("#16171B")
    private val surface = Color.parseColor("#1F2126")
    private val ink = Color.parseColor("#E6E8EC")
    private val secondary = Color.parseColor("#A4A9B3")
    private val accent = Color.parseColor("#9ECE6A")
    private val red = Color.parseColor("#E5676B")

    private lateinit var list: LinearLayout
    /** The computer being paired with, between the tap and its icons arriving. */
    private var waitingFor: Net.Computer? = null
    /** mDNS packets only reach an app holding one of these. */
    private var multicast: WifiManager.MulticastLock? = null

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()

    private fun text(value: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        text = value
        textSize = size
        setTextColor(color)
        setLineSpacing(0f, 1.15f)
        if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    }

    private fun rounded(color: Int, radius: Int) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = dp(radius).toFloat()
    }

    private fun add(view: View, top: Int) =
        list.addView(view, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(top) })

    private fun button(label: String, primary: Boolean, action: () -> Unit) = text(label, 15f, if (primary) Color.BLACK else ink, true).apply {
        gravity = Gravity.CENTER
        background = rounded(if (primary) accent else surface, 12)
        setPadding(0, dp(13), 0, dp(13))
        setOnClickListener { action() }
    }

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(28), dp(20), dp(28))
        }
        list = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(list, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        setContentView(ScrollView(this).apply {
            setBackgroundColor(backdrop)
            isFillViewport = true
            fitsSystemWindows = true
            addView(column)
        })
        LinkService.start(this)
    }

    override fun onResume() {
        super.onResume()
        visible = true
        current = this
        Net.onChange = ::render
        multicast = getSystemService(WifiManager::class.java)?.createMulticastLock("omalink-pairing")?.apply {
            setReferenceCounted(false)
            try { acquire() } catch (e: Exception) { }
        }
        render()
    }

    override fun onPause() {
        visible = false
        if (current === this) current = null
        Net.onChange = null
        try { multicast?.release() } catch (e: Exception) { }
        multicast = null
        super.onPause()
    }

    fun render() {
        list.removeAllViews()
        val verify = Net.verify
        val finished = Net.finished
        when {
            verify != null -> choose(verify)
            finished != null -> result(finished)
            waitingFor != null -> waiting(waitingFor!!)
            else -> computers()
        }
    }

    /** The three icons. */
    private fun choose(verify: org.json.JSONObject) {
        waitingFor = null
        val mirror = verify.optString("purpose") == "mirror"
        val name = verify.optString("name").ifEmpty { "your computer" }
        add(text(if (mirror) "$name wants to see your screen" else "Pair with $name", 24f, ink, true), 8)
        add(text(
            if (mirror) "If that's you, tap the icon $name is showing. Only the right one shares your screen; Android asks once more before anything is shown."
            else "Tap the icon $name is showing. If none of these match, someone else may be in between: tap Cancel.",
            14.5f, secondary,
        ), 10)
        val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val choices = verify.getJSONArray("choices")
        for (i in 0 until choices.length()) {
            val choice = choices.getJSONObject(i)
            val tile = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
                background = rounded(surface, 18)
                setPadding(0, dp(22), 0, dp(16))
                addView(text(choice.getString("emoji"), 46f, ink).apply { gravity = Gravity.CENTER })
                addView(text(choice.getString("name"), 13f, secondary).apply { gravity = Gravity.CENTER; setPadding(0, dp(6), 0, 0) })
                contentDescription = choice.getString("name")
                setOnClickListener { Net.choose(choice.getInt("index")) }
            }
            row.addView(tile, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply {
                if (i > 0) marginStart = dp(6)
                if (i < choices.length() - 1) marginEnd = dp(6)
            })
        }
        add(row, 26)
        add(button("Cancel", false) { Net.decline(); finish() }, 26)
    }

    private fun result(finished: org.json.JSONObject) {
        val ok = finished.optBoolean("ok")
        val mirror = finished.optString("purpose") == "mirror"
        val name = finished.optString("name").ifEmpty { "your computer" }
        add(text(
            when {
                ok && mirror -> "Approved"
                ok -> "Paired with $name"
                mirror -> "Screen not shared"
                else -> "Not paired"
            },
            24f, if (ok) accent else red, true,
        ), 8)
        add(text(
            when {
                ok && mirror -> "Android asks next whether to start sharing your screen."
                ok -> "This phone now reaches $name by itself whenever both have internet: on the same Wi-Fi or anywhere else. Files and screen sharing go straight between them, encrypted."
                else -> finished.optString("reason").ifEmpty { "The icon didn't match." }
            },
            14.5f, secondary,
        ), 10)
        add(button("Done", true) { Net.finished = null; finish() }, 24)
    }

    private fun waiting(computer: Net.Computer) {
        add(text("Look at ${computer.name}", 24f, ink, true), 8)
        add(text("Connecting… The computer shows an icon in a moment.", 14.5f, secondary), 10)
        add(button("Cancel", false) { waitingFor = null; render() }, 24)
    }

    /** Computers that are pairing nearby, and those already paired. */
    private fun computers() {
        add(text("Pair with a computer", 24f, ink, true), 8)
        add(text(
            "On your computer, open Omalink → Phone and press Pair a phone. It appears here when it's on the same Wi-Fi, or when this phone is connected to it over Bluetooth.",
            14.5f, secondary,
        ), 10)
        val found = synchronized(Net.computers) { Net.computers.values.toList() }
        val paired = Net.paired().map { it.first }.toSet()
        if (found.isEmpty()) {
            add(text("Looking for computers…", 14f, secondary).apply {
                gravity = Gravity.CENTER
                background = rounded(surface, 14)
                setPadding(dp(16), dp(22), dp(16), dp(22))
            }, 20)
        }
        for (computer in found) {
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                background = rounded(surface, 14)
                setPadding(dp(16), dp(15), dp(16), dp(15))
                addView(text(computer.name, 16f, ink, true))
                addView(text(
                    if (computer.id in paired) "Already paired · tap to pair again"
                    else if (computer.via == "bluetooth") "Found over Bluetooth" else "On this network",
                    13f, secondary,
                ))
                contentDescription = computer.name
                setOnClickListener {
                    waitingFor = computer
                    Net.pair(computer)
                    render()
                }
            }
            add(row, 12)
        }

        val known = Net.paired()
        if (known.isNotEmpty()) {
            add(text("Paired computers", 15f, ink, true), 26)
            for ((id, name) in known) {
                val row = LinearLayout(this).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    background = rounded(surface, 14)
                    setPadding(dp(16), dp(13), dp(12), dp(13))
                }
                val labels = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
                labels.addView(text(name.ifEmpty { "Computer" }, 15f, ink, true))
                labels.addView(text(if (Net.connected && Net.peer == name) "Connected · ${if (Net.route == "relay") "through a relay" else "direct"}" else "Not connected", 12.5f, secondary))
                row.addView(labels, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
                row.addView(text("Forget", 13.5f, red, true).apply {
                    setPadding(dp(12), dp(8), dp(4), dp(8))
                    setOnClickListener { Net.forget(id); render() }
                })
                add(row, 8)
            }
        }
    }
}
