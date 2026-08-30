package net.nodeinnet.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val AdwaitaDark = darkColorScheme(
    primary = AdwAccent,
    onPrimary = Color.White,
    secondary = AdwSuccess,
    onSecondary = Color.White,
    background = AdwWindowBg,
    onBackground = AdwText,
    surface = AdwViewBg,
    onSurface = AdwText,
    surfaceVariant = AdwCard,
    onSurfaceVariant = AdwTextDim,
    outline = Color(0xFF3A3A3A),
    outlineVariant = AdwBorder,
    error = AdwError,
    onError = Color.White,
)

private val AdwaitaLight = lightColorScheme(
    primary = AdwAccent,
    onPrimary = Color.White,
    secondary = AdwSuccess,
    background = AdwWindowBgLight,
    onBackground = AdwTextLight,
    surface = AdwViewBgLight,
    onSurface = AdwTextLight,
    surfaceVariant = AdwCardLight,
    onSurfaceVariant = AdwTextDimLight,
    outline = Color(0xFFCFCFCF),
    error = AdwError,
    onError = Color.White,
)

@Composable
fun NodeInNetTheme(
    darkTheme: Boolean = true,
    content: @Composable () -> Unit
) {
    MaterialTheme(
        colorScheme = if (darkTheme || isSystemInDarkTheme()) AdwaitaDark else AdwaitaLight,
        typography = Typography,
        content = content
    )
}
