package com.cenai.app

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
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
import java.io.FileInputStream
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

data class UpdateInfo(val version: String, val notes: String, val downloadUrl: String, val sha256: String)

class UpdateManager(private val context: Context) {
    private val client = OkHttpClient.Builder().followRedirects(true)
        .connectTimeout(20, TimeUnit.SECONDS).readTimeout(90, TimeUnit.SECONDS).build()
    private val owner = "C4RL-DANIEL"
    private val repo = "CenAi"

    suspend fun check(includePrerelease: Boolean = false): UpdateInfo? = withContext(Dispatchers.IO) {
        val request = Request.Builder().url("https://api.github.com/repos/$owner/$repo/releases?per_page=30")
            .header("Accept", "application/vnd.github+json").build()
        client.newCall(request).execute().use { response ->
            if (response.code == 404) return@withContext null
            if (!response.isSuccessful) throw IllegalStateException("Update check failed (HTTP ${response.code}).")
            val releases = org.json.JSONArray(response.body?.string().orEmpty())
            val release = (0 until releases.length()).map { releases.getJSONObject(it) }
                .firstOrNull { !it.optBoolean("draft", true) && (includePrerelease || !it.optBoolean("prerelease", false)) }
                ?: return@withContext null
            val assets = release.optJSONArray("assets") ?: return@withContext null
            val apk = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.optString("name").endsWith(".apk", true) } ?: return@withContext null
            val digest = apk.optString("digest", "").removePrefix("sha256:").lowercase()
            if (!digest.matches(Regex("[a-f0-9]{64}"))) {
                throw IllegalStateException("This release has no valid SHA-256 asset digest. Installation is blocked for safety.")
            }
            val latest = release.optString("tag_name").removePrefix("v")
            val current = context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
            if (compareVersions(latest, current) <= 0) return@withContext null
            UpdateInfo(latest, release.optString("body"), apk.optString("browser_download_url"), digest)
        }
    }

    private fun compareVersions(a: String, b: String): Int {
        fun parts(value: String): Pair<List<Int>, List<String>?> {
            val clean = value.removePrefix("v").substringBefore("+")
            val split = clean.split("-", limit = 2)
            val core = split[0].split(".").map { it.toIntOrNull() ?: 0 }
            return core to split.getOrNull(1)?.split(".")
        }
        val (aa, ap) = parts(a)
        val (bb, bp) = parts(b)
        for (i in 0 until maxOf(aa.size, bb.size)) {
            val diff = (aa.getOrNull(i) ?: 0).compareTo(bb.getOrNull(i) ?: 0)
            if (diff != 0) return diff
        }
        if (ap == null && bp == null) return 0
        if (ap == null) return 1
        if (bp == null) return -1
        for (i in 0 until maxOf(ap.size, bp.size)) {
            val x = ap.getOrNull(i) ?: return -1
            val y = bp.getOrNull(i) ?: return 1
            val xn = x.toIntOrNull()
            val yn = y.toIntOrNull()
            val diff = when {
                xn != null && yn != null -> xn.compareTo(yn)
                xn != null -> -1
                yn != null -> 1
                else -> x.compareTo(y)
            }
            if (diff != 0) return diff
        }
        return 0
    }

    suspend fun downloadAndInstall(info: UpdateInfo, onProgress: (Int) -> Unit) = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(info.downloadUrl).build()
        val apk = File(context.cacheDir, "cenai-update.apk")
        // Never reuse a previous or partially downloaded installer package.
        apk.delete()
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
        if (!apk.isFile || apk.length() <= 0L) {
            apk.delete()
            throw IllegalStateException("Downloaded APK is empty.")
        }
        val actualDigest = run {
            val digest = MessageDigest.getInstance("SHA-256")
            FileInputStream(apk).use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    digest.update(buffer, 0, count)
                }
            }
            digest.digest().joinToString("") { "%02x".format(it) }
        }
        if (!actualDigest.equals(info.sha256, ignoreCase = true)) {
            apk.delete()
            throw IllegalStateException("APK SHA-256 verification failed. The downloaded file was rejected.")
        }

        // SHA-256 protects transfer integrity; matching the installed signing certificate
        // prevents an update from replacing the app with an APK signed by another party.
        @Suppress("DEPRECATION")
        val signingFlags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        val archive = context.packageManager.getPackageArchiveInfo(apk.absolutePath, signingFlags) ?: run {
            apk.delete()
            throw IllegalStateException("Downloaded file is not a valid Android package.")
        }
        if (archive.packageName != context.packageName) {
            apk.delete()
            throw IllegalStateException("Update package ID does not match this CenAI installation.")
        }
        val installed = context.packageManager.getPackageInfo(context.packageName, signingFlags)
        fun fingerprints(info: PackageInfo): Set<String> {
            val signatures = if (Build.VERSION.SDK_INT >= 28) {
                info.signingInfo?.apkContentsSigners?.toList().orEmpty()
            } else {
                @Suppress("DEPRECATION")
                info.signatures?.toList().orEmpty()
            }
            return signatures.map { signature ->
                MessageDigest.getInstance("SHA-256").digest(signature.toByteArray())
                    .joinToString("") { byte -> "%02x".format(byte) }
            }.toSet()
        }
        if (fingerprints(installed).isEmpty() || fingerprints(archive) != fingerprints(installed)) {
            apk.delete()
            throw IllegalStateException("APK signing certificate does not match the installed CenAI app. Update blocked.")
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
