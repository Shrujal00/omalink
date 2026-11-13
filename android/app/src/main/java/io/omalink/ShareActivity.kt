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

    private fun sendFiles(files: List<Uri>) {
        done = true
        val service = LinkService.instance
        var sent = 0
        if (service != null && service.connected) {
            for (uri in files) if (service.transfers.offer(this, uri)) sent++
        }
        val message = when {
            service == null || !service.connected -> "Your computer isn't connected"
            sent == 0 -> "Nothing that can be sent"
            sent == 1 -> "Sending to your computer…"
            else -> "Sending $sent files to your computer…"
        }
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
        finish()
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
        val message = when {
            text.isNullOrEmpty() -> "Nothing to send"
            service == null || !service.connected -> "Your computer isn't connected"
            else -> {
                service.sendClipboard(text)
                "Sent to your computer"
            }
        }
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
        finish()
    }
}
