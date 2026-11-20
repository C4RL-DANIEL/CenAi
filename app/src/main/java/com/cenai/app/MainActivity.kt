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
        GuardianMonitor(applicationContext).installCrashRecorder()
        GuardianWorker.schedule(applicationContext)
        val vault = SecureKeyStore(applicationContext)
        setContent { CenAiScreen(vault) }
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
            val identity = CenAiIdentityStore(applicationContext, vault)
            val contextScope = BrainContextScope(identity.activeUserScopeId(), "chat")
            val result = GoogleGeminiOAuth(vault, contextScope).completeCallback(uri)
            Toast.makeText(this@MainActivity, if (result.isSuccess) "Google Gemini account connected." else (result.exceptionOrNull()?.message ?: "Google sign-in failed."), Toast.LENGTH_LONG).show()
        }
    }
}
