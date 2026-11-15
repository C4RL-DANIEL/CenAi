package com.cenai.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import org.json.JSONObject

/**
 * Narrow, foreground-only device actions. Every call must originate from the
 * explicit "Approve & run" button in the agent activity card.
 */
class DeviceActionManager(private val context: Context) {
    fun execute(name: String, args: JSONObject): String {
        val intent = when (name) {
            "open_url" -> {
                val raw = args.getString("url").trim()
                val uri = Uri.parse(raw)
                require(uri.scheme == "https" || uri.scheme == "http") { "Only HTTP and HTTPS links are allowed." }
                require(!uri.host.isNullOrBlank()) { "The link is invalid." }
                Intent(Intent.ACTION_VIEW, uri)
            }
            "open_dialer" -> {
                val number = args.getString("number").trim()
                require(number.matches(Regex("[+0-9*#(). -]{3,32}"))) { "The phone number contains unsupported characters." }
                Intent(Intent.ACTION_DIAL, Uri.fromParts("tel", number, null))
            }
            "share_text" -> {
                val text = args.getString("text")
                require(text.isNotBlank() && text.length <= 10_000) { "Share text must contain 1–10,000 characters." }
                Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
            }
            "open_app_settings" -> Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                Uri.parse("package:${context.packageName}"))
            else -> throw IllegalArgumentException("This device action is not available.")
        }
        val launch = if (name == "share_text") Intent.createChooser(intent, "Share with…") else intent
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(launch)
        return when (name) {
            "open_url" -> "Opened the link chooser."
            "open_dialer" -> "Opened the dialer. CenAI did not place a call."
            "share_text" -> "Opened Android's share sheet. Nothing was sent automatically."
            else -> "Opened Android app settings."
        }
    }
}
