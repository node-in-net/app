package net.nodeinnet.app.core

import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.Log
import android.webkit.MimeTypeMap
import java.io.File

object FileTransfers {
    private const val TAG = "FileTransfers"
    fun stageDir(context: Context): File =
        File(context.cacheDir, "transfers").apply { mkdirs() }

    fun staged(context: Context, transferId: String) = File(stageDir(context), transferId)
    fun publishToDownloads(context: Context, staged: File, displayName: String): Uri? {
        if (!staged.exists()) {
            Log.w(TAG, "staged file missing: ${staged.absolutePath}")
            return null
        }
        return try {
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, displayName)
                put(MediaStore.Downloads.MIME_TYPE, mimeOf(displayName))
                put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/node.in.net")
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val resolver = context.contentResolver
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                ?: return null
            resolver.openOutputStream(uri)?.use { out -> staged.inputStream().use { it.copyTo(out) } }
            resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
            staged.delete()
            uri
        } catch (e: Exception) {
            Log.e(TAG, "publish failed", e)
            null
        }
    }

     
    fun mimeOf(name: String): String {
        val ext = name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    }

     
    fun openWithViewer(context: Context, uri: Uri, displayName: String): Boolean {
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mimeOf(displayName))
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        return try {
            context.startActivity(intent)
            true
        } catch (e: ActivityNotFoundException) {
            Log.w(TAG, "no viewer for $displayName", e)
            false
        }
    }
     
    fun displayName(context: Context, uri: Uri): String {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { c ->
                if (c.moveToFirst() && !c.isNull(0)) return c.getString(0)
            }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    }

     
    fun copyToCache(context: Context, uri: Uri, name: String): File? {
        return try {
            val out = File(stageDir(context), "upload-$name")
            val input = context.contentResolver.openInputStream(uri) ?: return null
            input.use { out.outputStream().use { dst -> it.copyTo(dst) } }
            out
        } catch (e: Exception) {
            Log.e(TAG, "cache copy failed", e)
            null
        }
    }
}
