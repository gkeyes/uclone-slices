package com.uclone.slices.v2.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import top.yukonga.miuix.kmp.theme.MiuixTheme

/*
 * Cross-app design system v2 tokens (hard values shared by the family of apps). Screens and
 * components read spacing, shape, type, colour and motion only from here.
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
    val rowMinHeight = 56.dp
    val touchTarget = 48.dp
}

internal object Radius {
    val card = 24.dp
    val row = 16.dp
    val button = 16.dp

    /** Input fields and segmented controls. */
    val control = 14.dp

    /** Small elements: icon plates, inner tiles. */
    val small = 10.dp
    val chip = 999.dp
    val sheet = 28.dp
}

internal object TypeScale {
    val largeTitle = 30.sp
    val collapsedTitle = 17.sp
    val title = 20.sp
    val headline = 17.sp
    val body = 16.sp
    val subhead = 14.sp
    val sectionHeader = 13.sp
    val caption = 12.sp
}

/** Text emphasis is opacity on onSurface, never a hard-coded grey. */
internal object TextAlpha {
    const val Primary = .92f
    const val Secondary = .62f
    const val Tertiary = .42f
    const val Disabled = .38f
}

internal val TextPrimary: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.onSurface.copy(alpha = TextAlpha.Primary)
internal val TextSecondary: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.onSurface.copy(alpha = TextAlpha.Secondary)
internal val TextTertiary: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.onSurface.copy(alpha = TextAlpha.Tertiary)
internal val TextDisabled: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.onSurface.copy(alpha = TextAlpha.Disabled)

/** Tabular figures: every number that ticks or is compared across rows. */
internal val Tabular = TextStyle(fontFeatureSettings = "tnum")

/** Neutral surfaces from the spec; Miuix's own scheme is mapped onto the same values. */
internal object Surfaces {
    val page: Color
        @Composable @ReadOnlyComposable
        get() = if (isSystemInDarkTheme()) Color(0xFF000000) else Color(0xFFF5F6F8)
    val card: Color
        @Composable @ReadOnlyComposable
        get() = if (isSystemInDarkTheme()) Color(0xFF16171A) else Color(0xFFFFFFFF)
    val raised: Color
        @Composable @ReadOnlyComposable
        get() = if (isSystemInDarkTheme()) Color(0xFF212226) else Color(0xFFFFFFFF)
    val hairline: Color
        @Composable @ReadOnlyComposable
        get() = if (isSystemInDarkTheme()) Color.White.copy(alpha = .10f) else Color.Black.copy(alpha = .08f)
    val scrim: Color
        @Composable @ReadOnlyComposable
        get() = Color.Black.copy(alpha = if (isSystemInDarkTheme()) .50f else .32f)

    /** Press state layer: 6% black on light, 6% white on dark. */
    val stateLayer: Color
        @Composable @ReadOnlyComposable
        get() = if (isSystemInDarkTheme()) Color.White.copy(alpha = .06f) else Color.Black.copy(alpha = .06f)

    /** Fill of inputs and segmented tracks: a tint that differs from both page and card. */
    val control: Color
        @Composable @ReadOnlyComposable
        get() = MiuixTheme.colorScheme.onSurface.copy(alpha = .06f)
}

/** Accent: dynamic (Android 12+) or the spec fallback, as resolved by [SlicesTheme]. */
internal val Accent: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.primary
internal val OnAccent: Color
    @Composable @ReadOnlyComposable
    get() = MiuixTheme.colorScheme.onPrimary

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

/** Semantic tones for chips, banners and icon plates. */
internal enum class Tone { Neutral, Info, Success, Warning, Danger }

@Composable
@ReadOnlyComposable
internal fun Tone.container(): Color {
    val dark = isSystemInDarkTheme()
    return when (this) {
        Tone.Success -> if (dark) Color(0xFF17351F) else Color(0xFFE6F4EA)
        Tone.Warning -> if (dark) Color(0xFF3A2B10) else Color(0xFFFDF1DC)
        Tone.Danger -> if (dark) Color(0xFF3B1A1A) else Color(0xFFFCE8E8)
        Tone.Info -> if (dark) Color(0xFF172947) else Color(0xFFE5EEFD)
        Tone.Neutral -> if (dark) Color(0xFF26282D) else Color(0xFFEEF0F3)
    }
}

@Composable
@ReadOnlyComposable
internal fun Tone.onContainer(): Color {
    val dark = isSystemInDarkTheme()
    return when (this) {
        Tone.Success -> if (dark) Color(0xFF8FD9A8) else Color(0xFF1B6B34)
        Tone.Warning -> if (dark) Color(0xFFF5C36B) else Color(0xFF8A5300)
        Tone.Danger -> if (dark) Color(0xFFF4A7A3) else Color(0xFFB3261E)
        Tone.Info -> if (dark) Color(0xFFA9C5FA) else Color(0xFF1A57C9)
        Tone.Neutral -> if (dark) Color(0xFFC4C8CF) else Color(0xFF4A4F57)
    }
}
