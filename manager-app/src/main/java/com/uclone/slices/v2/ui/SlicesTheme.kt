package com.uclone.slices.v2.ui

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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/*
 * The Manager draws its own iOS 27 components (Components.kt, Glass.kt, liquid/). Material 3 is
 * only a bridge for static Text and Icon: their default text style and content colour come from
 * here. Colours follow the system light / dark setting; there is no theme or style picker.
 */

private val LightColors = materialLightColorScheme(
    primary = Color(0xFF007AFF),
    onPrimary = Color.White,
    background = Color(0xFFF2F2F7),
    onBackground = Color.Black,
    surface = Color.White,
    onSurface = Color.Black,
    surfaceVariant = Color(0xFFF2F2F7),
    onSurfaceVariant = Color(0xFF3C3C43),
    outline = Color(0xFFC6C6C8),
    outlineVariant = Color(0xFFE5E5EA),
    error = Color(0xFFFF3B30),
    onError = Color.White,
)

private val DarkColors = materialDarkColorScheme(
    primary = Color(0xFF0A84FF),
    onPrimary = Color.White,
    background = Color.Black,
    onBackground = Color.White,
    surface = Color(0xFF1C1C1E),
    onSurface = Color.White,
    surfaceVariant = Color(0xFF2C2C2E),
    onSurfaceVariant = Color(0xFFEBEBF5),
    outline = Color(0xFF38383A),
    outlineVariant = Color(0xFF2C2C2E),
    error = Color(0xFFFF453A),
    onError = Color.White,
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
        fontSize = 17.sp,
        lineHeight = 22.sp,
        fontWeight = FontWeight.Normal,
    ),
    bodyMedium = TextStyle(
        fontSize = 15.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.Normal,
    ),
    bodySmall = TextStyle(
        fontSize = 12.sp,
        lineHeight = 16.sp,
        fontWeight = FontWeight.Normal,
    ),
    labelLarge = TextStyle(
        fontSize = 15.sp,
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
    large = RoundedCornerShape(26.dp),
)

@Composable
internal fun SlicesTheme(content: @Composable () -> Unit) {
    val darkTheme = isSystemInDarkTheme()
    val materialColors = remember(darkTheme) { if (darkTheme) DarkColors else LightColors }
    MaterialTheme(
        colorScheme = materialColors,
        typography = SlicesTypography,
        shapes = SlicesShapes,
    ) {
        // Material 3 Text and Icon take their default colour from LocalContentColor, which is
        // black unless a Material Surface sets it. No Material Surface is used, so the theme root
        // provides the iOS label colour or every Text without an explicit colour stays black in
        // dark mode.
        CompositionLocalProvider(
            LocalContentColor provides TextPrimary,
            content = content,
        )
    }
}
