package com.cenai.app;

import android.app.NativeActivity;
import android.Manifest;
import android.content.pm.PackageManager;
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
