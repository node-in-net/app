package net.nodeinnet.app.core

import android.content.Context
import androidx.core.content.edit

class TokenManager(context: Context) {
    private val prefs = context.getSharedPreferences("nodeinnet_prefs", Context.MODE_PRIVATE)

    var refreshToken: String?
        get() = prefs.getString("refresh_token", null)
        set(value) = prefs.edit { putString("refresh_token", value) }

    var privateKey: String?
        get() = prefs.getString("private_key", null)
        set(value) = prefs.edit { putString("private_key", value) }

    var publicKey: String?
        get() = prefs.getString("public_key", null)
        set(value) = prefs.edit { putString("public_key", value) }
        
    var deviceId: String?
        get() = prefs.getString("device_id", null)
        set(value) = prefs.edit { putString("device_id", value) }
        
    fun clearAuth() {
        prefs.edit {
            remove("refresh_token")
            remove("account_name")
        }
    }
}
