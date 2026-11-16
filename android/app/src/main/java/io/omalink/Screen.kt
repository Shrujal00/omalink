package io.omalink

import android.content.Context
import android.os.Build
import android.util.DisplayMetrics
import android.view.WindowManager

/** The whole screen in pixels, the way it's being held right now. */
fun screenSize(context: Context): Pair<Int, Int> {
    val manager = context.getSystemService(WindowManager::class.java)
    if (Build.VERSION.SDK_INT >= 30) {
        val bounds = manager.maximumWindowMetrics.bounds
        return bounds.width() to bounds.height()
    }
    // Android 10 has no window metrics; the display's real size is the same thing.
    val metrics = DisplayMetrics()
    @Suppress("DEPRECATION")
    manager.defaultDisplay.getRealMetrics(metrics)
    return metrics.widthPixels to metrics.heightPixels
}
