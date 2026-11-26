package com.cenai.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent

class MainActivity : ComponentActivity() {
    private val notificationPermissionRequest = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { /* Permission is optional; background task notifications are best-effort when denied. */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        GuardianMonitor(applicationContext).installCrashRecorder()
        GuardianWorker.restoreEnabled(applicationContext)
        val vault = SecureKeyStore(applicationContext)
        setContent { CenAiScreen(vault) }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            notificationPermissionRequest.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
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
