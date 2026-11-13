package io.omalink

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Starts listening for the computer again after a reboot or an update. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        LinkService.start(context)
    }
}
