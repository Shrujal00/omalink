package io.omalink

import android.app.Activity
import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast

/**
 * Sends to the computer from the share sheet: text to its clipboard, files
 * to its Downloads. Also behind the "Send clipboard" button on the service
 * notification.
 *
 * Android only lets an app read the clipboard while it has the user's
 * attention, which is why this is an activity at all: it comes to the front
 * for an instant, reads, sends, and is gone.
 */
class ShareActivity : Activity() {
    companion object {
        const val SEND_CLIPBOARD = "io.omalink.SEND_CLIPBOARD"
    }

    private var done = false
    private var drop: Omadrop? = null

    override fun onDestroy() {
        LinkService.instance?.transfers?.let { if (it.watcher === drop) it.watcher = null }
        super.onDestroy()
    }

    @Deprecated("Back closes the sheet with its animation.")
    override fun onBackPressed() {
        drop?.close() ?: super.onBackPressed()
    }

    private fun computerName() = LinkService.instance?.peer?.ifEmpty { null } ?: "Your computer"

    private fun sheet(): Omadrop = Omadrop(this) {
        finish()
        @Suppress("DEPRECATION")
        overridePendingTransition(0, 0)
    }.also { drop = it }

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        when (intent?.action) {
            Intent.ACTION_SEND -> {
                @Suppress("DEPRECATION")
                val file = intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
                if (file != null) sendFiles(listOf(file)) else deliver(intent.getStringExtra(Intent.EXTRA_TEXT))
            }
            Intent.ACTION_SEND_MULTIPLE -> {
                @Suppress("DEPRECATION")
                sendFiles(intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM) ?: emptyList())
            }
        }
    }

    /** Omadrop: the sheet follows the files until they've landed. */
    private fun sendFiles(files: List<Uri>) {
        done = true
        val service = LinkService.instance
        val connected = service != null && service.connected && files.isNotEmpty()
        val drop = sheet()
        if (connected) service!!.transfers.watcher = drop
        drop.show(files, computerName(), connected)
        if (connected) for (uri in files) service!!.transfers.offer(this, uri)
    }

    override fun onWindowFocusChanged(focused: Boolean) {
        super.onWindowFocusChanged(focused)
        // The clipboard is readable only once the window actually has focus.
        if (focused && !done && intent?.action == SEND_CLIPBOARD) {
            val clip = getSystemService(ClipboardManager::class.java).primaryClip
            val text = if (clip != null && clip.itemCount > 0) clip.getItemAt(0).coerceToText(this)?.toString() else null
            deliver(text)
        }
    }

    private fun deliver(text: String?) {
        done = true
        val service = LinkService.instance
        if (text.isNullOrEmpty()) {
            Toast.makeText(this, "Nothing to send", Toast.LENGTH_SHORT).show()
            return finish()
        }
        val connected = service != null && service.connected
        if (connected) service!!.sendClipboard(text!!)
        sheet().showText(computerName(), connected)
    }
}
