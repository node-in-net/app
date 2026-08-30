package net.nodeinnet.app.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.IOException

sealed class AuthResult {
    data class Success(val accessToken: String, val refreshToken: String, val wsUrl: String = "", val turnCredentialsJson: String = "", val isPremium: Boolean = false, val turnRegion: String = "") : AuthResult()
    data class Error(val message: String, val isUnauthorized: Boolean = false) : AuthResult()
}

object AuthClient {
    const val BAD_REFRESH = "bad-refresh-response"

    private const val API_BASE = "https://node.in.net"
    private val client = OkHttpClient()
    private val JSON = "application/json; charset=utf-8".toMediaType()

    suspend fun setRegion(accessToken: String, region: String): String? =
        withContext(Dispatchers.IO) {
            val body = JSONObject().put("region", region).toString()
            val request = Request.Builder()
                .url("$API_BASE/account/region")
                .header("Authorization", "Bearer $accessToken")
                .post(body.toRequestBody(JSON))
                .build()
            try {
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) return@withContext null
                    val json = JSONObject(response.body?.string() ?: "")
                    json.optJSONObject("turn_credentials")?.toString()
                }
            } catch (e: Exception) {
                null
            }
        }

    suspend fun login(reqBodyJson: String): AuthResult = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("$API_BASE/account/login")
            .post(reqBodyJson.toRequestBody(JSON))
            .build()
            
        try {
            client.newCall(request).execute().use { response ->
                val bodyString = response.body?.string() ?: ""
                if (!response.isSuccessful) {
                    return@withContext AuthResult.Error("Login failed (HTTP ${response.code}): $bodyString", response.code == 401)
                }
                val jsonObject = JSONObject(bodyString)
                val accessToken = jsonObject.optString("access_token").ifEmpty { null }
                val refreshToken = jsonObject.optString("refresh_token").ifEmpty { null }
                val wsUrl = jsonObject.optString("ws_url", "")
                val turnObj = jsonObject.optJSONObject("turn")
                val turnCredentialsJson = turnObj?.toString() ?: ""
                val isPremium = jsonObject.optInt("premium", 0) == 1
                
                if (accessToken != null && refreshToken != null) {
                    AuthResult.Success(accessToken, refreshToken, wsUrl, turnCredentialsJson, isPremium, jsonObject.optString("turn_region", ""))
                } else {
                    AuthResult.Error("Invalid response format from server.")
                }
            }
        } catch (e: Exception) {
            AuthResult.Error("Network error: ${e.message}")
        }
    }

    suspend fun registerDevice(accessToken: String, reqBodyJson: String): String? = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("$API_BASE/account/devices")
            .header("Authorization", "Bearer $accessToken")
            .post(reqBodyJson.toRequestBody(JSON))
            .build()
            
        try {
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) return@withContext null
                val bodyString = response.body?.string()
                if (bodyString != null && bodyString.isNotEmpty()) {
                    val jsonObject = JSONObject(bodyString)
                    jsonObject.optString("id").ifEmpty { null }
                } else null
            }
        } catch (e: Exception) {
            null
        }
    }

    suspend fun refresh(refreshToken: String, region: String): AuthResult = withContext(Dispatchers.IO) {
        val reqJson = JSONObject()
            .put("refresh_token", refreshToken)
            .put("region", region)
            .toString()
        val request = Request.Builder()
            .url("$API_BASE/account/refresh_token")
            .post(reqJson.toRequestBody(JSON))
            .build()
            
        try {
            client.newCall(request).execute().use { response ->
                val bodyString = response.body?.string() ?: ""
                if (!response.isSuccessful) {
                    val isUnauthorized = response.code == 401 || response.code == 403 || response.code == 400
                    return@withContext AuthResult.Error("Refresh failed (HTTP ${response.code}): $bodyString", isUnauthorized)
                }
                val jsonObject = JSONObject(bodyString)
                val accessToken = jsonObject.optString("access_token").ifEmpty { null }
                val newRefreshToken = jsonObject.optString("refresh_token").ifEmpty { null }
                val wsUrl = jsonObject.optString("ws_url", "")
                val turnObj = jsonObject.optJSONObject("turn")
                val turnCredentialsJson = turnObj?.toString() ?: ""
                val isPremium = jsonObject.optInt("premium", 0) == 1
                
                if (accessToken != null && newRefreshToken != null) {
                    AuthResult.Success(accessToken, newRefreshToken, wsUrl, turnCredentialsJson, isPremium, jsonObject.optString("turn_region", ""))
                } else {
                    AuthResult.Error(BAD_REFRESH)
                }
            }
        } catch (e: Exception) {
            AuthResult.Error("Network error during refresh: ${e.message}")
        }
    }
}
