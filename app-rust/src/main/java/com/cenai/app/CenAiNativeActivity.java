package com.cenai.app;

import android.app.NativeActivity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.UUID;

public final class CenAiNativeActivity extends NativeActivity {
    public static final int REQUEST_ATTACHMENT = 8101;
    public static final int REQUEST_BACKUP = 8102;
    public static final int REQUEST_CREATE_BACKUP = 8103;
    private static final long MAX_IMPORT_BYTES = 100L * 1024L * 1024L;

    private static native void nativeOnFilePicked(int requestCode, String path, String mimeType, String displayName, String error);
    private static native void nativeOnDocumentCreated(int requestCode, String uri);

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

    public void createBackupDocument() {
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("application/json");
        intent.putExtra(Intent.EXTRA_TITLE, "CenAI-workspace-backup.cenai");
        launchPicker(intent, REQUEST_CREATE_BACKUP);
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
        if (requestCode != REQUEST_ATTACHMENT && requestCode != REQUEST_BACKUP && requestCode != REQUEST_CREATE_BACKUP) {
            return;
        }
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            if (requestCode == REQUEST_CREATE_BACKUP) {
                nativeOnDocumentCreated(requestCode, "");
            } else {
                nativeOnFilePicked(requestCode, "", "", "", "File selection was cancelled.");
            }
            return;
        }
        Uri uri = data.getData();
        if (requestCode == REQUEST_CREATE_BACKUP) {
            nativeOnDocumentCreated(requestCode, uri.toString());
            return;
        }
        String mime = getContentResolver().getType(uri);
        String name = displayName(uri);
        try {
            File cache = new File(getCacheDir(), "cenai-imports");
            if (!cache.exists() && !cache.mkdirs()) {
                throw new java.io.IOException("Could not create the private import cache.");
            }
            File destination = new File(cache, UUID.randomUUID().toString() + ".bin");
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

    public boolean writeFileToUri(String uriString, String sourcePath) {
        try (InputStream input = new FileInputStream(sourcePath);
             OutputStream output = getContentResolver().openOutputStream(Uri.parse(uriString), "wt")) {
            if (output == null) return false;
            byte[] buffer = new byte[32 * 1024];
            int count;
            while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
            output.flush();
            return true;
        } catch (Exception error) {
            android.util.Log.e("CenAI", "Could not write backup document", error);
            return false;
        }
    }
}
