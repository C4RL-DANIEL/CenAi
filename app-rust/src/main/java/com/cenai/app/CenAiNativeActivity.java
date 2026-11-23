package com.cenai.app;

import android.app.NativeActivity;
import android.Manifest;
import android.content.pm.PackageManager;
import android.content.pm.PackageInfo;
import android.content.SharedPreferences;
import android.security.keystore.KeyProperties;
import android.util.Base64;

import javax.crypto.Cipher;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;
import java.security.KeyStore;
import android.os.Build;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import android.provider.Settings;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.UUID;

public final class CenAiNativeActivity extends NativeActivity {
    private static boolean notificationPermissionRequested = false;
    public static final int REQUEST_ATTACHMENT = 8101;
    public static final int REQUEST_BACKUP = 8102;
    private static final long MAX_IMPORT_BYTES = 100L * 1024L * 1024L;

    private static native void nativeOnFilePicked(int requestCode, String path, String mimeType, String displayName, String error);
    private static native void nativeOnPlatformActionResult(String name, boolean success, String message);
    private static native void nativeOnOAuthCallback(String callback);
    private static native void nativeOnAutomationReminder(String notificationTag);

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        GuardianRuntimeBridge.install(this);
        GuardianWorker.schedule(this);
        // Register Rust cognitive work with Android's persistent scheduler.
        // Rust checks persisted user consent and safely no-ops without an eligible plan.
        BackgroundTaskBridge.schedule(this);
        // Reconcile OS reminder jobs with the Rust-owned automation schedule.
        AutomationReminderBridge.scheduleRefresh(this);
        handleOAuthIntent(getIntent());
        handleAutomationReminderIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleOAuthIntent(intent);
        handleAutomationReminderIntent(intent);
    }

    private void handleOAuthIntent(Intent intent) {
        if (intent == null || intent.getData() == null) return;
        Uri uri = intent.getData();
        if ("com.cenai.app".equals(uri.getScheme()) && "/oauth2redirect".equals(uri.getPath())) {
            nativeOnOAuthCallback(uri.toString());
        }
    }

    /** Routes a generic WorkManager reminder to the matching signed-in Rust automation. */
    private void handleAutomationReminderIntent(Intent intent) {
        if (intent == null || intent.getData() == null) return;
        Uri uri = intent.getData();
        if (!"cenai".equals(uri.getScheme()) || !"automation-reminder".equals(uri.getHost())) return;
        String tag = uri.getLastPathSegment();
        if (tag != null && tag.matches("cenai_automation_[0-9a-f]{64}")) {
            nativeOnAutomationReminder(tag);
        }
    }

    /**
     * Exports the previous Kotlin app's encrypted account and provider secrets
     * for a one-time in-process migration into the Rust-owned Keystore vault.
     * Values are never logged or written to disk in plaintext.
     */
    public String exportLegacySecureStore() {
        org.json.JSONObject result = new org.json.JSONObject();
        org.json.JSONObject keys = new org.json.JSONObject();
        try {
            SharedPreferences prefs = getSharedPreferences("cenai_private", MODE_PRIVATE);
            KeyStore store = KeyStore.getInstance("AndroidKeyStore");
            store.load(null);
            SecretKey legacyKey = (SecretKey) store.getKey("cenai_aes_key_v1", null);
            if (legacyKey == null) return "{}";
            result.put("access_token", decryptLegacy(prefs.getString("secret_supabase_access_token", null), legacyKey));
            result.put("refresh_token", decryptLegacy(prefs.getString("secret_supabase_refresh_token", null), legacyKey));
            result.put("account_email", prefs.getString("supabase_account_email", ""));
            result.put("email_verified", "true".equals(prefs.getString("cenai_email_verified", "false")));
            for (String provider : new String[]{"openai", "deepseek", "claude", "gemini", "custom"}) {
                String encrypted = prefs.getString("api_key_" + provider, null);
                String value = decryptLegacy(encrypted, legacyKey);
                if (!value.isEmpty()) keys.put(provider, value);
            }
            String defaultKey = decryptLegacy(prefs.getString("api_key", null), legacyKey);
            if (!defaultKey.isEmpty() && !keys.has("openai")) keys.put("openai", defaultKey);
            // Migrate only durable OAuth credentials. An in-flight PKCE state
            // is intentionally discarded so an old browser redirect cannot be replayed.
            String googleClientId = prefs.getString("google_oauth_client_id", "");
            if (!googleClientId.isEmpty()) keys.put("gemini_oauth_client_id", googleClientId);
            String geminiAccess = decryptLegacy(prefs.getString("secret_gemini_oauth_access_token", null), legacyKey);
            String geminiRefresh = decryptLegacy(prefs.getString("secret_gemini_oauth_refresh_token", null), legacyKey);
            if (!geminiAccess.isEmpty()) keys.put("gemini_oauth_access_token", geminiAccess);
            if (!geminiRefresh.isEmpty()) keys.put("gemini_oauth_refresh_token", geminiRefresh);
            long expiryMillis = 0L;
            try { expiryMillis = Long.parseLong(prefs.getString("gemini_oauth_expires_at", "0")); }
            catch (NumberFormatException ignored) { }
            if (expiryMillis > 0L) keys.put("gemini_oauth_expires_at_ms", Long.toString(expiryMillis));
            result.put("provider_keys", keys);
            return result.toString();
        } catch (Exception error) {
            android.util.Log.w("CenAI", "Legacy secure-store migration export unavailable; original data was retained.");
            return "{}";
        }
    }

    private String decryptLegacy(String encoded, SecretKey key) throws Exception {
        if (encoded == null || encoded.isEmpty()) return "";
        byte[] packed = Base64.decode(encoded, Base64.NO_WRAP);
        if (packed.length <= 12) return "";
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(128, packed, 0, 12));
        return new String(cipher.doFinal(packed, 12, packed.length - 12), java.nio.charset.StandardCharsets.UTF_8);
    }

    /**
     * Exports the legacy conversation JSON without mutating or deleting it.
     * Rust validates every identifier, role, message and size before import.
     */
    public String exportLegacyConversationHistory() {
        try {
            SharedPreferences prefs = getSharedPreferences("cenai_history", MODE_PRIVATE);
            return prefs.getString("conversations", "[]");
        } catch (Exception error) {
            android.util.Log.w("CenAI", "Legacy conversation history export unavailable; original data was retained.");
            return "[]";
        }
    }

    /** Called only after Rust confirms the imported vault data is durable. */
    public void clearLegacySecureStoreAfterMigration() {
        try {
            SharedPreferences prefs = getSharedPreferences("cenai_private", MODE_PRIVATE);
            boolean preferencesCommitted = prefs.edit()
                .remove("secret_supabase_access_token")
                .remove("secret_supabase_refresh_token")
                .remove("cenai_email_verified")
                .remove("api_key")
                .remove("api_key_openai")
                .remove("api_key_deepseek")
                .remove("api_key_claude")
                .remove("api_key_gemini")
                .remove("api_key_custom")
                .remove("secret_google_oauth_verifier")
                .remove("secret_google_oauth_state")
                .remove("secret_gemini_oauth_access_token")
                .remove("secret_gemini_oauth_refresh_token")
                .remove("google_oauth_client_id")
                .remove("gemini_oauth_expires_at")
                .commit();
            if (!preferencesCommitted) {
                android.util.Log.w("CenAI", "Legacy secure-store cleanup was not durable; the old Keystore key was retained.");
                return;
            }
            KeyStore store = KeyStore.getInstance("AndroidKeyStore");
            store.load(null);
            store.deleteEntry("cenai_aes_key_v1");
        } catch (Exception error) {
            android.util.Log.w("CenAI", "Legacy secure-store cleanup could not be completed.");
        }
    }

    public String getInstalledVersionName() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (Exception error) {
            return "0.0.0";
        }
    }

    /** Opens only Google's official OAuth authorization endpoint. */
    public boolean openGoogleOAuth(String value) {
        try {
            Uri uri = Uri.parse(value);
            if (!"https".equals(uri.getScheme()) || !"accounts.google.com".equals(uri.getHost())
                || !"/o/oauth2/v2/auth".equals(uri.getPath()) || uri.getUserInfo() != null) {
                throw new IllegalArgumentException("Google authorization URL was rejected.");
            }
            startActivity(new Intent(Intent.ACTION_VIEW, uri));
            return true;
        } catch (Exception error) {
            return false;
        }
    }

    /** Executes only a native-reviewed, one-time agent action. Calls are never placed. */
    public boolean performAgentAction(String action, String value) {
        try {
            Intent intent;
            String result;
            switch (action) {
                case "open_url": {
                    Uri uri = Uri.parse(value);
                    String scheme = uri.getScheme();
                    if (uri.getHost() == null || !("https".equalsIgnoreCase(scheme) || "http".equalsIgnoreCase(scheme))
                        || uri.getUserInfo() != null) {
                        throw new IllegalArgumentException("Only public HTTP(S) links can be opened.");
                    }
                    intent = new Intent(Intent.ACTION_VIEW, uri);
                    result = "Opened the approved link.";
                    break;
                }
                case "open_dialer": {
                    if (value == null || !value.matches("[0-9+*#(). \\-]{4,32}")) {
                        throw new IllegalArgumentException("The phone number is invalid.");
                    }
                    intent = new Intent(Intent.ACTION_DIAL, Uri.fromParts("tel", value, null));
                    result = "Opened the dialer. No call was placed.";
                    break;
                }
                case "share_text": {
                    if (value == null || value.trim().isEmpty() || value.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 65536) {
                        throw new IllegalArgumentException("The shared text is empty or too large.");
                    }
                    intent = new Intent(Intent.ACTION_SEND);
                    intent.setType("text/plain");
                    intent.putExtra(Intent.EXTRA_TEXT, value);
                    intent = Intent.createChooser(intent, "Share with…");
                    result = "Opened the Android share sheet.";
                    break;
                }
                case "open_app_settings": {
                    if (value != null && !value.isEmpty()) throw new IllegalArgumentException("Unexpected settings arguments.");
                    intent = new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                        Uri.fromParts("package", getPackageName(), null));
                    result = "Opened CenAI app settings.";
                    break;
                }
                default:
                    throw new IllegalArgumentException("This Android action is not supported.");
            }
            startActivity(intent);
            nativeOnPlatformActionResult(action, true, result);
            return true;
        } catch (Exception error) {
            String message = error.getMessage() == null ? "Android could not open the requested action." : error.getMessage();
            nativeOnPlatformActionResult(action == null ? "unknown" : action, false, message);
            return false;
        }
    }

    /**
     * Opens Android's package installer only after verifying the package ID and
     * matching the APK signing certificate to the currently installed app.
     */
    public boolean installVerifiedUpdate(String absolutePath) {
        try {
            File updateDirectory = new File(getFilesDir(), "updates").getCanonicalFile();
            File apk = new File(absolutePath).getCanonicalFile();
            if (!apk.isFile() || !apk.getPath().startsWith(updateDirectory.getPath() + File.separator)) {
                throw new IllegalArgumentException("Update file is outside CenAI's private update directory.");
            }
            @SuppressWarnings("deprecation")
            int flags = Build.VERSION.SDK_INT >= 28
                ? PackageManager.GET_SIGNING_CERTIFICATES : PackageManager.GET_SIGNATURES;
            PackageManager manager = getPackageManager();
            PackageInfo archive = manager.getPackageArchiveInfo(apk.getAbsolutePath(), flags);
            if (archive == null || !getPackageName().equals(archive.packageName)) {
                throw new IllegalArgumentException("Downloaded APK is not a valid CenAI package.");
            }
            PackageInfo installed = manager.getPackageInfo(getPackageName(), flags);
            java.util.Set<String> expected = signerFingerprints(installed);
            java.util.Set<String> actual = signerFingerprints(archive);
            if (expected.isEmpty() || !expected.equals(actual)) {
                throw new SecurityException("APK signing certificate does not match this installation. Update blocked.");
            }
            if (Build.VERSION.SDK_INT >= 26 && !manager.canRequestPackageInstalls()) {
                Intent settings = new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:" + getPackageName())).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                startActivity(settings);
                return false;
            }
            Uri uri = androidx.core.content.FileProvider.getUriForFile(
                this, getPackageName() + ".fileprovider", apk);
            Intent install = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(install);
            return true;
        } catch (Exception error) {
            android.util.Log.e("CenAI", "Verified update installation was blocked", error);
            return false;
        }
    }

    private java.util.Set<String> signerFingerprints(PackageInfo info) throws Exception {
        android.content.pm.Signature[] signatures;
        if (Build.VERSION.SDK_INT >= 28) {
            if (info.signingInfo == null) return java.util.Collections.emptySet();
            signatures = info.signingInfo.getApkContentsSigners();
        } else {
            @SuppressWarnings("deprecation")
            android.content.pm.Signature[] legacy = info.signatures;
            signatures = legacy;
        }
        java.util.Set<String> result = new java.util.HashSet<>();
        if (signatures == null) return result;
        java.security.MessageDigest digest = java.security.MessageDigest.getInstance("SHA-256");
        for (android.content.pm.Signature signature : signatures) {
            byte[] hash = digest.digest(signature.toByteArray());
            StringBuilder hex = new StringBuilder(hash.length * 2);
            for (byte value : hash) hex.append(String.format(java.util.Locale.ROOT, "%02x", value & 0xff));
            result.add(hex.toString());
        }
        return result;
    }

    /** Called only when the user enables scheduled reminders in CenAI. */
    public void requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= 33 && !notificationPermissionRequested
            && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            notificationPermissionRequested = true;
            requestPermissions(new String[] { Manifest.permission.POST_NOTIFICATIONS }, 8120);
        }
    }

    public void openAttachmentPicker() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {
            "image/*", "application/pdf", "text/*", "audio/*", "video/*"
        });
        launchPicker(intent, REQUEST_ATTACHMENT);
    }

    public void openBackupPicker() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {
            "application/json", "application/octet-stream"
        });
        launchPicker(intent, REQUEST_BACKUP);
    }

    private void launchPicker(Intent intent, int requestCode) {
        try {
            startActivityForResult(intent, requestCode);
        } catch (ActivityNotFoundException error) {
            nativeOnFilePicked(requestCode, "", "", "", "No compatible Android file picker is available.");
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_ATTACHMENT && requestCode != REQUEST_BACKUP) {
            return;
        }
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            nativeOnFilePicked(requestCode, "", "", "", "File selection was cancelled.");
            return;
        }
        Uri uri = data.getData();
        String mime = getContentResolver().getType(uri);
        String name = displayName(uri);
        File destination = null;
        try {
            File cache = new File(getCacheDir(), "cenai-imports");
            if (!cache.exists() && !cache.mkdirs()) {
                throw new java.io.IOException("Could not create the private import cache.");
            }
            destination = new File(cache, UUID.randomUUID().toString() + ".bin");
            long total = 0;
            try (InputStream input = getContentResolver().openInputStream(uri);
                 OutputStream output = new FileOutputStream(destination)) {
                if (input == null) throw new java.io.IOException("Android could not open the selected file.");
                byte[] buffer = new byte[32 * 1024];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    total += count;
                    if (total > MAX_IMPORT_BYTES) {
                        throw new java.io.IOException("Selected file exceeds the 100 MB import limit.");
                    }
                    output.write(buffer, 0, count);
                }
                output.flush();
            }
            nativeOnFilePicked(requestCode, destination.getAbsolutePath(), mime == null ? "application/octet-stream" : mime,
                name == null ? "attachment" : name, "");
        } catch (Exception error) {
            if (destination != null && destination.exists()) {
                // Never leave a partial import behind after cancellation, size-limit or I/O failure.
                //noinspection ResultOfMethodCallIgnored
                destination.delete();
            }
            nativeOnFilePicked(requestCode, "", mime == null ? "" : mime, name == null ? "" : name,
                error.getMessage() == null ? "Could not import the selected file." : error.getMessage());
        }
    }

    private String displayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[] { OpenableColumns.DISPLAY_NAME }, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) return cursor.getString(column);
            }
        } catch (Exception ignored) { }
        return uri.getLastPathSegment();
    }

}
