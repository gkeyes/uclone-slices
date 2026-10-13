package com.uclone.slices.v2.ui

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Container and content colour of one semantic [Tone]. */
@Immutable
internal data class ToneColors(
    val container: Color,
    val onContainer: Color,
)

/**
 * Every colour of the iOS 27 look, resolved for one appearance. [SlicesTheme] provides the light
 * or dark set through [LocalSlicesColors]; the tokens in Tokens.kt ([Surfaces], [Label], [Accent],
 * [Tone.container] ...) read it from there instead of checking the system setting themselves, so a
 * different set (an in-app appearance switch, a glass-specific palette) only has to be provided.
 */
@Immutable
internal data class SlicesColors(
    val isDark: Boolean,
    /** iOS label colour; text emphasis is opacity on it ([TextAlpha]). */
    val label: Color,
    /** systemGroupedBackground. */
    val page: Color,
    /** secondarySystemGroupedBackground. */
    val card: Color,
    val raised: Color,
    /** Tint drawn over refracted glass (title-bar buttons, button groups). */
    val glassTint: Color,
    /** Heavier tint for floating panels (sheets, alerts, notices) so their text stays readable. */
    val panelTint: Color,
    val hairline: Color,
    val scrim: Color,
    /** Press state layer. */
    val stateLayer: Color,
    /** Quiet fill of inputs, search field, segmented track and unselected chips. */
    val control: Color,
    /** Off track of a switch. */
    val switchOff: Color,
    /** Raised capsule of the segmented control. */
    val segmentIndicator: Color,
    /** The iOS system blue. */
    val accent: Color,
    val onAccent: Color,
    /** iOS system green for switches. */
    val systemGreen: Color,
    /** iOS system red for destructive glass buttons. */
    val systemRed: Color,
    val neutral: ToneColors,
    val info: ToneColors,
    val success: ToneColors,
    val warning: ToneColors,
    val danger: ToneColors,
) {
    fun tone(tone: Tone): ToneColors = when (tone) {
        Tone.Neutral -> neutral
        Tone.Info -> info
        Tone.Success -> success
        Tone.Warning -> warning
        Tone.Danger -> danger
    }
}

internal val LightSlicesColors = SlicesColors(
    isDark = false,
    label = Color.Black,
    page = Color(0xFFF2F2F7),
    card = Color(0xFFFFFFFF),
    raised = Color(0xFFFFFFFF),
    glassTint = Color.White.copy(alpha = .42f),
    panelTint = Color.White.copy(alpha = .70f),
    hairline = Color.Black.copy(alpha = .08f),
    scrim = Color.Black.copy(alpha = .32f),
    stateLayer = Color.Black.copy(alpha = .06f),
    control = Color(0xFF767680).copy(alpha = .12f),
    switchOff = Color(0xFFE9E9EA),
    segmentIndicator = Color.White,
    accent = Color(0xFF007AFF),
    onAccent = Color.White,
    systemGreen = Color(0xFF34C759),
    systemRed = Color(0xFFFF3B30),
    neutral = ToneColors(Color(0xFFEEF0F3), Color(0xFF4A4F57)),
    info = ToneColors(Color(0xFFE5EEFD), Color(0xFF1A57C9)),
    success = ToneColors(Color(0xFFE6F4EA), Color(0xFF1B6B34)),
    warning = ToneColors(Color(0xFFFDF1DC), Color(0xFF8A5300)),
    danger = ToneColors(Color(0xFFFCE8E8), Color(0xFFB3261E)),
)

internal val DarkSlicesColors = SlicesColors(
    isDark = true,
    label = Color.White,
    page = Color(0xFF000000),
    card = Color(0xFF1C1C1E),
    raised = Color(0xFF2C2C2E),
    glassTint = Color(0xFF1C1C1E).copy(alpha = .42f),
    panelTint = Color(0xFF242428).copy(alpha = .72f),
    hairline = Color.White.copy(alpha = .10f),
    scrim = Color.Black.copy(alpha = .50f),
    stateLayer = Color.White.copy(alpha = .06f),
    control = Color(0xFF767680).copy(alpha = .24f),
    switchOff = Color(0xFF39393D),
    segmentIndicator = Color(0xFF636366),
    accent = Color(0xFF0A84FF),
    onAccent = Color.White,
    systemGreen = Color(0xFF30D158),
    systemRed = Color(0xFFFF453A),
    neutral = ToneColors(Color(0xFF26282D), Color(0xFFC4C8CF)),
    info = ToneColors(Color(0xFF172947), Color(0xFFA9C5FA)),
    success = ToneColors(Color(0xFF17351F), Color(0xFF8FD9A8)),
    warning = ToneColors(Color(0xFF3A2B10), Color(0xFFF5C36B)),
    danger = ToneColors(Color(0xFF3B1A1A), Color(0xFFF4A7A3)),
)

/** The colours of the current appearance; provided by [SlicesTheme]. */
internal val LocalSlicesColors = staticCompositionLocalOf { LightSlicesColors }
