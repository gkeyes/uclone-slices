package com.uclone.slices.v2.ui

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

internal val SlicesSuccess = Color(0xFF16885A)
internal val SlicesSuccessContainer = Color(0xFFE3F5EC)
internal val SlicesWarning = Color(0xFFE18428)
internal val SlicesWatermelon = Color(0xFFD83B50)

private val SlicesColorScheme = lightColorScheme(
    primary = Color(0xFF5E6AD2),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE7E9FF),
    onPrimaryContainer = Color(0xFF242854),
    secondary = Color(0xFF60647B),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE7E8F2),
    onSecondaryContainer = Color(0xFF252735),
    background = Color(0xFFF6F7FB),
    onBackground = Color(0xFF1C1D23),
    surface = Color.White,
    onSurface = Color(0xFF1C1D23),
    surfaceVariant = Color(0xFFF0F1F6),
    onSurfaceVariant = Color(0xFF666A78),
    outline = Color(0xFFD9DCE7),
    outlineVariant = Color(0xFFE8EAF1),
    error = Color(0xFFBA1A1A),
    onError = Color.White,
    errorContainer = Color(0xFFFFDAD6),
    onErrorContainer = Color(0xFF410002),
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
    MaterialTheme(
        colorScheme = SlicesColorScheme,
        typography = SlicesTypography,
        shapes = SlicesShapes,
        content = content,
    )
}
