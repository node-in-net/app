package net.nodeinnet.app.core

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.util.Log
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

object AppUpdater {
    private const val TAG = "AppUpdater"

    suspend fun downloadAndInstall(
        urlParam: String,
        expectedMd5: String,
        context: Context,
    ): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                val fullUrl = if (urlParam.startsWith("/")) {
                    "https://node.in.net$urlParam"
                } else if (!urlParam.startsWith("http")) {
                    "https://node.in.net/download/$urlParam"
                } else {
                    urlParam
                }

                Log.i(TAG, "Starting download from: $fullUrl")
                val url = URL(fullUrl)
                val connection = url.openConnection() as HttpURLConnection
                connection.requestMethod = "GET"
                connection.connect()

                if (connection.responseCode != HttpURLConnection.HTTP_OK) {
                    Log.e(TAG, "Server returned HTTP ${connection.responseCode} ${connection.responseMessage}")
                    return@withContext false
                }

                val cacheDir = context.externalCacheDir ?: context.cacheDir
                val fileExt = fullUrl.substringAfterLast('.', "apk")
                val fileName = "nodeinnet-update.${fileExt}"
                val outputFile = File(cacheDir, fileName)

                if (outputFile.exists()) {
                    outputFile.delete()
                }

                val input = connection.inputStream
                val output = FileOutputStream(outputFile)

                val digest = MessageDigest.getInstance("MD5")
                val data = ByteArray(4096)
                var total: Long = 0
                var count: Int
                while (input.read(data).also { count = it } != -1) {
                    total += count.toLong()
                    digest.update(data, 0, count)
                    output.write(data, 0, count)
                }

                output.flush()
                output.close()
                input.close()

                val expected = expectedMd5.trim()
                if (expected.isNotEmpty()) {
                    val got = digest.digest().joinToString("") { "%02x".format(it) }
                    if (!got.equals(expected, ignoreCase = true)) {
                        Log.e(TAG, "Checksum mismatch: expected $expected, got $got. Refusing to install.")
                        outputFile.delete()
                        return@withContext false
                    }
                }

                Log.i(TAG, "Download complete. File saved to: ${outputFile.absolutePath} (Size: $total bytes)")

                installApk(outputFile, context)
                true
            } catch (e: Exception) {
                Log.e(TAG, "Error downloading or installing update", e)
                false
            }
        }
    }

    private fun installApk(apkFile: File, context: Context) {
        try {
            val authority = "${context.packageName}.provider"
            val apkUri: Uri = FileProvider.getUriForFile(context, authority, apkFile)

            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(apkUri, "application/vnd.android.package-archive")
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION
            }

            Log.i(TAG, "Starting installation intent for $apkUri")
            context.startActivity(intent)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start installation intent", e)
        }
    }
}
