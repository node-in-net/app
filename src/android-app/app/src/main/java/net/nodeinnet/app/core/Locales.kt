package net.nodeinnet.app.core

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import java.util.Locale

object Locales {
    private const val PREFS = "nodeinnet_prefs"
    private const val KEY = "ui.language"

    const val SYSTEM = ""

    val CHOICES = listOf(
        "en" to "English",
        "pl" to "Polski",
        "cs" to "Čeština",
        "sk" to "Slovenčina",
        "de" to "Deutsch",
        "es" to "Español",
        "uk" to "Українська",
        "it" to "Italiano",
        "fr" to "Français",
        "ro" to "Română",
        "hu" to "Magyar",
        "be" to "Беларуская",
        "ru" to "Русский",
        "bg" to "Български",
        "sr" to "Српски",
    )

    fun saved(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, SYSTEM) ?: SYSTEM

    fun save(context: Context, tag: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY, tag)
            .apply()
    }

    fun wrap(base: Context): Context {
        if (saved(base) == SYSTEM) return base
        val locale = current(base)
        Locale.setDefault(locale)
        val config = Configuration(base.resources.configuration)
        config.setLocale(locale)
        return base.createConfigurationContext(config)
    }

    fun apply(context: Context) {
        val locale = current(context)
        Locale.setDefault(locale)
        val res = context.applicationContext.resources
        val config = Configuration(res.configuration)
        config.setLocale(locale)
        @Suppress("DEPRECATION")
        res.updateConfiguration(config, res.displayMetrics)
    }

    private fun current(context: Context): Locale {
        val tag = saved(context)
        return if (tag == SYSTEM) {
            Resources.getSystem().configuration.locales[0]
        } else {
            Locale.forLanguageTag(tag)
        }
    }
}
