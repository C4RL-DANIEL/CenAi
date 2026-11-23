package com.cenai.app

import android.os.Bundle
import android.widget.Toast
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Render the stable Compose UI immediately. Optional background services
        // must never hold the first frame hostage or prevent the app from opening.
        val vault = SecureKeyStore(applicationContext)
        setContent { CenAiScreen(vault) }

        val guardian = GuardianMonitor(applicationContext)
        runCatching { guardian.installCrashRecorder() }
            .onFailure { android.util.Log.w("CenAI", "Guardian crash recorder unavailable; continuing UI startup.", it) }
        runCatching { GuardianWorker.schedule(applicationContext) }
            .onFailure { android.util.Log.w("CenAI", "Guardian scheduling unavailable; continuing UI startup.", it) }

        handleOAuthCallback(intent, vault)
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleOAuthCallback(intent, SecureKeyStore(applicationContext))
    }

    private fun handleOAuthCallback(intent: android.content.Intent?, vault: SecureKeyStore) {
        val uri = intent?.data ?: return
        if (uri.scheme != "com.cenai.app" || uri.path != "/oauth2redirect") return
        lifecycleScope.launch {
            val result = GoogleGeminiOAuth(vault).completeCallback(uri)
            Toast.makeText(this@MainActivity, if (result.isSuccess) "Google Gemini account connected." else (result.exceptionOrNull()?.message ?: "Google sign-in failed."), Toast.LENGTH_LONG).show()
        }
    }
}
