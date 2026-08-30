package net.nodeinnet.app.core

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.core.net.toUri
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

data class Share(val name: String, val path: String)

object Shares {
    private const val PREFS = "nodeinnet_prefs"
    private const val KEY = "fs_shares"
    val suggested: List<Share>
        get() = listOf(
            Environment.DIRECTORY_DCIM,
            Environment.DIRECTORY_PICTURES,
            Environment.DIRECTORY_DOWNLOADS,
            Environment.DIRECTORY_DOCUMENTS,
            Environment.DIRECTORY_MOVIES,
            Environment.DIRECTORY_MUSIC,
        ).map { Share(it, File(Environment.getExternalStorageDirectory(), it).absolutePath) }

    fun load(context: Context): List<Share> {
        val raw = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, null)
            ?: return emptyList()
        return try {
            val arr = JSONArray(raw)
            (0 until arr.length()).mapNotNull { i ->
                val o = arr.optJSONObject(i) ?: return@mapNotNull null
                Share(o.optString("name"), o.optString("path"))
            }
        } catch (_: Exception) {
            emptyList()
        }
    }

    fun save(context: Context, shares: List<Share>) {
        val arr = JSONArray()
        shares.forEach { arr.put(JSONObject().put("name", it.name).put("path", it.path)) }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString(KEY, arr.toString()).apply()
    }

     
    fun configJson(shares: List<Share>): String {
        val arr = JSONArray()
        shares.forEach { arr.put(JSONObject().put("name", it.name).put("path", it.path)) }
        return JSONObject().put("shares", arr).toString()
    }

    fun hasAllFilesAccess(): Boolean = Environment.isExternalStorageManager()

     
    fun allFilesAccessIntent(context: Context) =
        Intent(
            Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
            "package:${context.packageName}".toUri(),
        )

    fun pathFromTreeUri(uri: Uri): String? {
        val docId = try {
            DocumentsContract.getTreeDocumentId(uri)
        } catch (_: Exception) {
            return null
        }
        val parts = docId.split(':', limit = 2)
        val volume = parts.getOrNull(0) ?: return null
        val relative = parts.getOrNull(1).orEmpty()
        val base = if (volume == "primary") {
            Environment.getExternalStorageDirectory().absolutePath
        } else {
            "/storage/$volume"
        }
        val path = if (relative.isEmpty()) base else "$base/$relative"
        return if (File(path).isDirectory) path else null
    }

     
    fun uniqueName(existing: List<Share>, wanted: String): String {
        val taken = existing.map { it.name }.toSet()
        if (wanted !in taken) return wanted
        var n = 2
        while ("$wanted ($n)" in taken) n++
        return "$wanted ($n)"
    }
}
