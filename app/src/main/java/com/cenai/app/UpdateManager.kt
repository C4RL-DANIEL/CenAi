package com.cenai.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.TimeUnit

data class UpdateInfo(val version: String, val notes: String, val downloadUrl: String)

class UpdateManager(private val context: Context) {
    private val client = OkHttpClient.Builder().followRedirects(true)
        .connectTimeout(20, TimeUnit.SECONDS).readTimeout(90, TimeUnit.SECONDS).build()
    private val owner = "C4RL-DANIEL"
    private val repo = "CenAi"

    suspend fun check(): UpdateInfo? = withContext(Dispatchers.IO) {
        val request = Request.Builder().url("https://api.github.com/repos/$owner/$repo/releases/latest")
            .header("Accept", "application/vnd.github+json").build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw IllegalStateException("Update check failed (HTTP ${response.code}).")
            val release = JSONObject(response.body?.string().orEmpty())
            val assets = release.optJSONArray("assets") ?: return@withContext null
            val apk = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.optString("name").endsWith(".apk", true) } ?: return@withContext null
            val latest = release.optString("tag_name").removePrefix("v")
            val current = context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
            if (compareVersions(latest, current) <= 0) return@withContext null
            UpdateInfo(latest, release.optString("body"), apk.optString("browser_download_url"))
        }
    }

    private fun compareVersions(a: String, b: String): Int {
        val aa = a.split(".", "-", "+").map { it.toIntOrNull() ?: 0 }
        val bb = b.split(".", "-", "+").map { it.toIntOrNull() ?: 0 }
        for (i in 0 until maxOf(aa.size, bb.size)) {
            val diff = (aa.getOrNull(i) ?: 0).compareTo(bb.getOrNull(i) ?: 0)
            if (diff != 0) return diff
        }
        return 0
    }

    suspend fun downloadAndInstall(info: UpdateInfo, onProgress: (Int) -> Unit) = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(info.downloadUrl).build()
        val apk = File(context.cacheDir, "cenai-update.apk")
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw IllegalStateException("APK download failed (HTTP ${response.code}).")
            val body = response.body ?: throw IllegalStateException("Empty APK download.")
            val total = body.contentLength()
            body.byteStream().use { input ->
                FileOutputStream(apk).use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var copied = 0L
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        output.write(buffer, 0, count)
                        copied += count
                        if (total > 0) onProgress(((copied * 100) / total).toInt().coerceIn(0, 100))
                    }
                    output.fd.sync()
                }
            }
        }
        withContext(Dispatchers.Main) {
            if (Build.VERSION.SDK_INT >= 26 && !context.packageManager.canRequestPackageInstalls()) {
                val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(intent)
                throw IllegalStateException("Allow CenAI to install updates, then tap Install update again.")
            }
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", apk)
            val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
        }
    }
}
