package com.uclone.slices.v2.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/*
 * Cross-app design system v2 tokens, iOS 27 edition (the DualSimRouting design draft): grouped
 * surfaces, 26dp cards, ~52dp rows, capsule controls and Liquid Glass chrome. Screens and
 * components read spacing, shape, type, colour and motion only from here. One fixed look that
 * follows the system light / dark setting; there is no theme or style picker. Colour values live
 * in SlicesColors.kt and reach these tokens through [LocalSlicesColors].
 */

/** 4dp grid. */
internal object Space {
    val xxs = 4.dp
    val xs = 8.dp

    /** Card gap and spacing between elements inside a group. */
    val sm = 12.dp

    /** Page gutter and card padding. */
    val md = 16.dp

    /** Gap between sections. */
    val lg = 24.dp
    val xl = 32.dp
    val gutter = md
    val cardPadding = md

    /** Section headers and footnotes align with the text inside cards: gutter + card padding. */
    val textInset = gutter + cardPadding

    /** iOS grouped row height. */
    val rowMinHeight = 52.dp
    val touchTarget = 48.dp

    /** Round glass buttons of the title bar (back, actions). */
    val glassButton = 44.dp

    /** Floating title bar height under the status bar. */
    val topBar = 56.dp

    /** Inset of floating glass panels (sheets) from the screen edges. */
    val panelInset = 12.dp
}

/** iOS 27 geometry: 26dp cards, capsule buttons and controls, 34dp floating glass panels. */
internal object Radius {
    val card = 26.dp
    val row = 16.dp

    /** Buttons are capsules. */
    val button = 999.dp

    /** Search field and segmented control: capsules. */
    val control = 999.dp

    /** Text inputs: a rounded field like the iOS grouped text field. */
    val field = 14.dp

    /** Small elements: app-icon fallbacks, inner tiles. */
    val small = 10.dp

    /** Settings-style icon tiles. */
    val icon = 8.dp
    val chip = 999.dp

    /** Floating glass sheets and alerts. */
    val sheet = 34.dp

    /** Glass menus. */
    val menu = 22.dp
}

internal object TypeScale {
    val largeTitle = 34.sp
    val collapsedTitle = 17.sp
    val title = 20.sp
    val headline = 17.sp
    val body = 17.sp
    val subhead = 15.sp
    val sectionHeader = 13.sp
    val caption = 12.sp
}

/** Text emphasis is opacity on the label colour, never a hard-coded grey. */
internal object TextAlpha {
    const val Primary = .92f
    const val Secondary = .62f
    const val Tertiary = .42f
    const val Disabled = .38f
}

/** The colours of the current appearance, as provided by [SlicesTheme]. */
internal val ThemeColors: SlicesColors
    @Composable @ReadOnlyComposable
    get() = LocalSlicesColors.current

/** iOS label colour: black on light, white on dark. */
internal val Label: Color
    @Composable @ReadOnlyComposable
    get() = ThemeColors.label

internal val TextPrimary: Color
    @Composable @ReadOnlyComposable
    get() = Label.copy(alpha = TextAlpha.Primary)
internal val TextSecondary: Color
    @Composable @ReadOnlyComposable
    get() = Label.copy(alpha = TextAlpha.Secondary)
internal val TextTertiary: Color
    @Composable @ReadOnlyComposable
    get() = Label.copy(alpha = TextAlpha.Tertiary)
internal val TextDisabled: Color
    @Composable @ReadOnlyComposable
    get() = Label.copy(alpha = TextAlpha.Disabled)

/** Tabular figures: every number that ticks or is compared across rows. */
internal val Tabular = TextStyle(fontFeatureSettings = "tnum")

/** iOS grouped surfaces (systemGroupedBackground / secondarySystemGroupedBackground). */
internal object Surfaces {
    val page: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.page
    val card: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.card
    val raised: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.raised

    /** Tint drawn over refracted glass (title-bar buttons, button groups). */
    val glassTint: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.glassTint

    /** Heavier tint for floating panels (sheets, alerts, notices) so their text stays readable. */
    val panelTint: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.panelTint
    val hairline: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.hairline
    val scrim: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.scrim

    /** Press state layer: 6% black on light, 6% white on dark. */
    val stateLayer: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.stateLayer

    /** Quiet fill of inputs, search field, segmented track and unselected chips (tertiarySystemFill). */
    val control: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.control

    /** Off track of a switch. */
    val switchOff: Color
        @Composable @ReadOnlyComposable
        get() = ThemeColors.switchOff
}

/** Accent: the iOS system blue (one fixed look; no wallpaper colour). */
internal val Accent: Color
    @Composable @ReadOnlyComposable
    get() = ThemeColors.accent
internal val OnAccent: Color
    @Composable @ReadOnlyComposable
    get() = ThemeColors.onAccent

/** iOS system green for switches. */
internal val SystemGreen: Color
    @Composable @ReadOnlyComposable
    get() = ThemeColors.systemGreen

/** iOS system red for destructive glass buttons. */
internal val SystemRed: Color
    @Composable @ReadOnlyComposable
    get() = ThemeColors.systemRed

internal object Motion {
    /** Spatial: position and size. */
    fun <T> spatial(visibilityThreshold: T? = null): SpringSpec<T> = spring(0.86f, 380f, visibilityThreshold)

    /** Effects: colour and opacity. */
    fun <T> effects(): SpringSpec<T> = spring(1f, 600f)
    fun <T> pressIn(): SpringSpec<T> = spring(1f, 1500f)
    fun <T> pressRelease(): SpringSpec<T> = spring(0.62f, 420f)
    fun <T> page(visibilityThreshold: T? = null): SpringSpec<T> = spring(1f, 380f, visibilityThreshold)
    fun <T> sheetEnter(): SpringSpec<T> = spring(0.9f, 380f)
    fun <T> dialogEnter(): SpringSpec<T> = spring(0.9f, 500f)
    val ExitEasing = CubicBezierEasing(0.3f, 0f, 1f, 1f)
    fun <T> exit(): TweenSpec<T> = tween(220, easing = ExitEasing)
    const val ScrimFadeMs = 220

    val offset: SpringSpec<IntOffset> = spatial(IntOffset(1, 1))
    val size: SpringSpec<IntSize> = spatial(IntSize(1, 1))
    val pageOffset: SpringSpec<IntOffset> = page(IntOffset(1, 1))

    const val PressedScale = .965f
    const val PressedScaleRow = .985f
    const val StaggerMs = 26L
    const val StaggerMaxItems = 8
    val StaggerOffset = 16.dp
    val PageOffset = 30.dp
    const val DialogInitialScale = .94f
}

/** Semantic tones for chips, banners and icon tiles. */
internal enum class Tone { Neutral, Info, Success, Warning, Danger }

@Composable
@ReadOnlyComposable
internal fun Tone.container(): Color = ThemeColors.tone(this).container

@Composable
@ReadOnlyComposable
internal fun Tone.onContainer(): Color = ThemeColors.tone(this).onContainer

/** Gradient of a settings-style icon tile (top-left light to bottom-right deep), as in iOS Settings. */
internal fun Tone.tileGradient(): List<Color> = when (this) {
    Tone.Success -> listOf(Color(0xFF4CD964), Color(0xFF1E9E45))
    Tone.Warning -> listOf(Color(0xFFFFB340), Color(0xFFF28500))
    Tone.Danger -> listOf(Color(0xFFFF6B6B), Color(0xFFE5372B))
    Tone.Info -> listOf(Color(0xFF4A9BFF), Color(0xFF0A5BD6))
    Tone.Neutral -> listOf(Color(0xFFA1A1A6), Color(0xFF6C6C70))
}
