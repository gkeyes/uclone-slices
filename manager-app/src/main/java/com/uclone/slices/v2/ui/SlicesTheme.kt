package com.uclone.slices.v2.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme as materialDarkColorScheme
import androidx.compose.material3.lightColorScheme as materialLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.res.colorResource
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import top.yukonga.miuix.kmp.theme.MiuixTheme
import top.yukonga.miuix.kmp.theme.darkColorScheme as miuixDarkColorScheme
import top.yukonga.miuix.kmp.theme.lightColorScheme as miuixLightColorScheme

// Spec fallback accent when the platform has no dynamic colour (Android 11 and below).
private val FallbackAccentLight = Color(0xFF3478F6)
private val FallbackAccentDark = Color(0xFF5A93FF)

private data class AccentPair(val accent: Color, val onAccent: Color)

/** Android 12+ dynamic accent (system_accent1 tone 40 / 80), otherwise the spec fallback. */
@Composable
private fun rememberAccent(dark: Boolean): AccentPair =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        if (dark) {
            AccentPair(
                colorResource(android.R.color.system_accent1_200),
                colorResource(android.R.color.system_accent1_800),
            )
        } else {
            AccentPair(colorResource(android.R.color.system_accent1_600), Color.White)
        }
    } else if (dark) {
        AccentPair(FallbackAccentDark, Color.White)
    } else {
        AccentPair(FallbackAccentLight, Color.White)
    }

private fun lightMaterialColors(accent: AccentPair) = materialLightColorScheme(
    primary = accent.accent,
    onPrimary = accent.onAccent,
    primaryContainer = Color(0xFFE5EEFD),
    onPrimaryContainer = Color(0xFF1A57C9),
    secondary = Color(0xFF4A4F57),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFEEF0F3),
    onSecondaryContainer = Color(0xFF4A4F57),
    background = Color(0xFFF5F6F8),
    onBackground = Color(0xFF1C1D23),
    surface = Color.White,
    onSurface = Color(0xFF1C1D23),
    surfaceVariant = Color(0xFFEEF0F3),
    onSurfaceVariant = Color(0xFF5C606A),
    outline = Color(0xFFD2D2D6),
    outlineVariant = Color(0xFFE6E6E9),
    error = Color(0xFFB3261E),
    onError = Color.White,
    errorContainer = Color(0xFFFCE8E8),
    onErrorContainer = Color(0xFFB3261E),
)

private fun darkMaterialColors(accent: AccentPair) = materialDarkColorScheme(
    primary = accent.accent,
    onPrimary = accent.onAccent,
    primaryContainer = Color(0xFF172947),
    onPrimaryContainer = Color(0xFFA9C5FA),
    secondary = Color(0xFFC4C8CF),
    onSecondary = Color(0xFF26282D),
    secondaryContainer = Color(0xFF26282D),
    onSecondaryContainer = Color(0xFFC4C8CF),
    background = Color.Black,
    onBackground = Color(0xFFF1F1F3),
    surface = Color(0xFF16171A),
    onSurface = Color(0xFFF1F1F3),
    surfaceVariant = Color(0xFF212226),
    onSurfaceVariant = Color(0xFFB4B4BC),
    outline = Color(0xFF525257),
    outlineVariant = Color(0xFF353539),
    error = Color(0xFFF4A7A3),
    onError = Color(0xFF3B1A1A),
    errorContainer = Color(0xFF3B1A1A),
    onErrorContainer = Color(0xFFF4A7A3),
)

// Miuix page / card levels are mapped onto the spec surfaces so Miuix popups, fields and
// switches sit on the same values as the custom components.
private fun lightMiuixColors(accent: AccentPair) = miuixLightColorScheme(
    primary = accent.accent,
    onPrimary = accent.onAccent,
    primaryVariant = accent.accent,
    onPrimaryVariant = Color(0xFFE5EEFD),
    error = Color(0xFFB3261E),
    errorContainer = Color(0xFFFCE8E8),
    background = Color(0xFFF5F6F8),
    surface = Color(0xFFF5F6F8),
    surfaceVariant = Color.White,
    surfaceContainer = Color.White,
    surfaceContainerHigh = Color(0xFFEEF0F3),
    surfaceContainerHighest = Color(0xFFE4E5E9),
    outline = Color(0xFFD2D2D6),
    dividerLine = Color(0x14000000),
    windowDimming = Color.Black.copy(alpha = .32f),
)

private fun darkMiuixColors(accent: AccentPair) = miuixDarkColorScheme(
    primary = accent.accent,
    onPrimary = accent.onAccent,
    primaryVariant = accent.accent,
    error = Color(0xFFF4A7A3),
    errorContainer = Color(0xFF3B1A1A),
    background = Color.Black,
    surface = Color.Black,
    surfaceVariant = Color(0xFF16171A),
    surfaceContainer = Color(0xFF16171A),
    surfaceContainerHigh = Color(0xFF212226),
    surfaceContainerHighest = Color(0xFF2A2B30),
    dividerLine = Color(0x1AFFFFFF),
    windowDimming = Color.Black.copy(alpha = .50f),
)

private val SlicesTypography = Typography(
    headlineMedium = TextStyle(
        fontSize = 28.sp,
        lineHeight = 34.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.35).sp,
    ),
    titleLarge = TextStyle(
        fontSize = 22.sp,
        lineHeight = 28.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.2).sp,
    ),
    titleMedium = TextStyle(
        fontSize = 17.sp,
        lineHeight = 22.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.1).sp,
    ),
    bodyLarge = TextStyle(
        fontSize = 16.sp,
        lineHeight = 24.sp,
        fontWeight = FontWeight.Normal,
    ),
    bodyMedium = TextStyle(
        fontSize = 14.sp,
        lineHeight = 21.sp,
        fontWeight = FontWeight.Normal,
    ),
    bodySmall = TextStyle(
        fontSize = 12.sp,
        lineHeight = 18.sp,
        fontWeight = FontWeight.Normal,
    ),
    labelLarge = TextStyle(
        fontSize = 14.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.SemiBold,
    ),
    labelMedium = TextStyle(
        fontSize = 12.sp,
        lineHeight = 16.sp,
        fontWeight = FontWeight.SemiBold,
    ),
)

private val SlicesShapes = Shapes(
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp),
)

@Composable
internal fun SlicesTheme(content: @Composable () -> Unit) {
    val darkTheme = isSystemInDarkTheme()
    val accent = rememberAccent(darkTheme)
    val miuixColors = remember(darkTheme, accent) {
        if (darkTheme) darkMiuixColors(accent) else lightMiuixColors(accent)
    }
    val materialColors = remember(darkTheme, accent) {
        if (darkTheme) darkMaterialColors(accent) else lightMaterialColors(accent)
    }
    MiuixTheme(
        colors = miuixColors,
    ) {
        MaterialTheme(
            colorScheme = materialColors,
            typography = SlicesTypography,
            shapes = SlicesShapes,
        ) {
            // Material 3 Text and Icon take their default colour from LocalContentColor, which is
            // black unless a Material Surface sets it. Screens sit on MIUIX containers that only
            // set MIUIX's own content colour, so without this every Text without an explicit
            // colour stays black in dark mode.
            CompositionLocalProvider(
                LocalContentColor provides MiuixTheme.colorScheme.onSurface,
                content = content,
            )
        }
    }
}
