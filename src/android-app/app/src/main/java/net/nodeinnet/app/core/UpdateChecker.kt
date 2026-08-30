package net.nodeinnet.app.core

import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject

data class UpdatePackage(
    val appType: String,
    val buildType: String,
    val version: String,
    val url: String,
    val md5: String,
)

object UpdateChecker {
    private const val TAG = "UpdateChecker"
    private const val MANIFEST_URL = "https://node.in.net/downloads/updates.json"
    private const val APP_TYPE = "mobile"
    private const val BUILD_TYPE = "apk"

    private val client = OkHttpClient()

     
    fun currentVersion(context: Context): String =
        try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "0.0.0"
        } catch (e: PackageManager.NameNotFoundException) {
            Log.e(TAG, "own package not found", e)
            "0.0.0"
        }

     
    suspend fun findUpdate(context: Context): UpdatePackage? = withContext(Dispatchers.IO) {
        val installed = currentVersion(context)
        try {
            val body = client.newCall(Request.Builder().url(MANIFEST_URL).build()).execute().use {
                if (!it.isSuccessful) {
                    Log.w(TAG, "manifest returned HTTP ${it.code}")
                    return@withContext null
                }
                it.body?.string()
            } ?: return@withContext null

            
            
            val trimmed = body.trimStart()
            val packages = if (trimmed.startsWith("[")) {
                JSONArray(body)
            } else {
                JSONObject(body).optJSONArray("packages") ?: JSONArray()
            }
            for (i in 0 until packages.length()) {
                val o = packages.getJSONObject(i)
                if (o.optString("app_type") != APP_TYPE) continue
                if (o.optString("build_type") != BUILD_TYPE) continue
                val version = o.optString("version")
                if (!isNewer(installed, version)) return@withContext null
                return@withContext UpdatePackage(
                    appType = APP_TYPE,
                    buildType = BUILD_TYPE,
                    version = version,
                    url = o.optString("url"),
                    md5 = o.optString("md5"),
                )
            }
            null
        } catch (e: Exception) {
            Log.w(TAG, "update check failed: ${e.message}")
            null
        }
    }

     
    fun isNewer(current: String, latest: String): Boolean {
        val cur = current.split('.').map { it.toIntOrNull() ?: 0 }
        val lat = latest.split('.').map { it.toIntOrNull() ?: 0 }
        for (i in 0 until maxOf(cur.size, lat.size)) {
            val c = cur.getOrElse(i) { 0 }
            val l = lat.getOrElse(i) { 0 }
            if (l > c) return true
            if (c > l) return false
        }
        return false
    }
}
