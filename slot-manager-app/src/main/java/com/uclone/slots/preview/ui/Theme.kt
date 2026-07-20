package com.uclone.slots.preview.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val PreviewIndigo = Color(0xFF5E6AD2)
val SafeGreen = Color(0xFF16885A)
val WarningOrange = Color(0xFFE18428)
val AppBackground = Color(0xFFF6F7FB)

private val PreviewColors = lightColorScheme(
    primary = PreviewIndigo,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE7E9FF),
    onPrimaryContainer = Color(0xFF242B72),
    secondary = SafeGreen,
    tertiary = WarningOrange,
    background = AppBackground,
    surface = Color.White,
    surfaceVariant = Color(0xFFEFF1F6),
    outlineVariant = Color(0xFFDDE0E9),
    error = Color(0xFFBA1A1A),
)

@Composable
fun SlotsTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = PreviewColors,
        typography = MaterialTheme.typography,
        content = content,
    )
}
