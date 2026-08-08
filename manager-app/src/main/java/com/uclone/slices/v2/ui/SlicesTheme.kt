package com.uclone.slices.v2.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme as materialDarkColorScheme
import androidx.compose.material3.lightColorScheme as materialLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import top.yukonga.miuix.kmp.theme.MiuixTheme
import top.yukonga.miuix.kmp.theme.darkColorScheme as miuixDarkColorScheme
import top.yukonga.miuix.kmp.theme.lightColorScheme as miuixLightColorScheme

internal val SlicesSuccess = Color(0xFF16885A)
internal val SlicesSuccessContainer = Color(0xFFE3F5EC)
internal val SlicesWarning = Color(0xFFE18428)
internal val SlicesWatermelon = Color(0xFFD83B50)
private val SlicesBlue = Color(0xFF3482FF)

private val LightMaterialColors = materialLightColorScheme(
    primary = SlicesBlue,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE8F1FF),
    onPrimaryContainer = Color(0xFF14345D),
    secondary = Color(0xFF60647B),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFECECF0),
    onSecondaryContainer = Color(0xFF252735),
    background = Color(0xFFF7F7F8),
    onBackground = Color(0xFF1C1D23),
    surface = Color.White,
    onSurface = Color(0xFF1C1D23),
    surfaceVariant = Color(0xFFF1F1F3),
    onSurfaceVariant = Color(0xFF666A78),
    outline = Color(0xFFD2D2D6),
    outlineVariant = Color(0xFFE6E6E9),
    error = Color(0xFFE94634),
    onError = Color.White,
    errorContainer = Color(0xFFFFECE9),
    onErrorContainer = Color(0xFF6A1710),
)

private val DarkMaterialColors = materialDarkColorScheme(
    primary = Color(0xFF5A9AFF),
    onPrimary = Color.White,
    primaryContainer = Color(0xFF183A67),
    onPrimaryContainer = Color(0xFFD8E8FF),
    secondary = Color(0xFFBEC2D8),
    onSecondary = Color(0xFF292C3B),
    secondaryContainer = Color(0xFF35363B),
    onSecondaryContainer = Color(0xFFF0F0F4),
    background = Color(0xFF111113),
    onBackground = Color(0xFFF1F1F3),
    surface = Color(0xFF1C1C1E),
    onSurface = Color(0xFFF1F1F3),
    surfaceVariant = Color(0xFF2A2A2D),
    onSurfaceVariant = Color(0xFFB4B4BC),
    outline = Color(0xFF525257),
    outlineVariant = Color(0xFF353539),
    error = Color(0xFFFF6257),
    onError = Color.White,
    errorContainer = Color(0xFF461A16),
    onErrorContainer = Color(0xFFFFDAD5),
)

private val LightMiuixColors = miuixLightColorScheme(
    primary = SlicesBlue,
    onPrimary = Color.White,
    primaryVariant = SlicesBlue,
    onPrimaryVariant = Color(0xFFD8E8FF),
    error = Color(0xFFE94634),
    errorContainer = Color(0xFFFFECE9),
    background = Color.White,
    surface = Color(0xFFF7F7F8),
    surfaceVariant = Color.White,
    surfaceContainer = Color.White,
    surfaceContainerHigh = Color(0xFFECECEE),
    surfaceContainerHighest = Color(0xFFE4E4E7),
    outline = Color(0xFFD2D2D6),
    dividerLine = Color(0xFFE6E6E9),
)

private val DarkMiuixColors = miuixDarkColorScheme(
    primary = Color(0xFF5A9AFF),
    primaryVariant = Color(0xFF4D91FF),
    error = Color(0xFFFF6257),
    errorContainer = Color(0xFF461A16),
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
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(20.dp),
)

@Composable
internal fun SlicesTheme(content: @Composable () -> Unit) {
    val darkTheme = isSystemInDarkTheme()
    MiuixTheme(
        colors = if (darkTheme) DarkMiuixColors else LightMiuixColors,
    ) {
        MaterialTheme(
            colorScheme = if (darkTheme) DarkMaterialColors else LightMaterialColors,
            typography = SlicesTypography,
            shapes = SlicesShapes,
            content = content,
        )
    }
}
