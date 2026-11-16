package io.omalink

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.os.Bundle
import org.json.JSONObject

/**
 * Asks Android for the screen, after the icon check passed. Android insists
 * on asking every session, from a screen of the app's own, which this is:
 * it shows nothing but Android's prompt and goes away.
 */
class MirrorActivity : Activity() {
    companion object {
        private const val ASK = 9
        private const val EXTRA_SIZE = "max_size"

        fun start(context: Context, maxSize: Int) {
            val intent = Intent(context, MirrorActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                .putExtra(EXTRA_SIZE, maxSize)
            try { context.startActivity(intent) } catch (e: Exception) { }
        }
    }

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        if (saved != null) return
        val manager = getSystemService(MediaProjectionManager::class.java)
        try {
            startActivityForResult(manager.createScreenCaptureIntent(), ASK)
        } catch (e: Exception) {
            declined()
        }
    }

    override fun onActivityResult(code: Int, result: Int, data: Intent?) {
        super.onActivityResult(code, result, data)
        if (code == ASK && result == RESULT_OK && data != null) {
            MirrorService.start(this, result, data, intent.getIntExtra(EXTRA_SIZE, 1600))
        } else {
            declined()
        }
        finish()
    }

    private fun declined() {
        LinkService.instance?.send(message("mirror_ended", JSONObject().put("reason", "Screen sharing was declined on the phone.")))
        finish()
    }
}
