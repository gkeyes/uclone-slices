package com.uclone.slices.v2.ui

import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kyant.backdrop.Backdrop
import com.kyant.backdrop.backdrops.LayerBackdrop
import com.kyant.backdrop.backdrops.layerBackdrop
import com.kyant.backdrop.backdrops.rememberLayerBackdrop
import com.kyant.backdrop.drawBackdrop
import com.kyant.backdrop.effects.blur
import com.kyant.backdrop.effects.lens
import com.kyant.backdrop.effects.vibrancy
import com.kyant.backdrop.highlight.Highlight
import com.kyant.backdrop.shadow.Shadow

/*
 * iOS 27 Liquid Glass on top of Kyant0/AndroidLiquidGlass (io.github.kyant0:backdrop), the same
 * approach as DualSimRouting. Page content is recorded with [glassSource]; chrome drawn with
 * [glassSurface] samples that recording and refracts it through an AGSL lens.
 *
 * Chrome must never sample a recording it is itself part of. [LocalGlassBackdrop] therefore only
 * holds a backdrop where the composable reading it is drawn outside that backdrop's source: the
 * root provides its page recording to the sheets, dialog and notice it draws above the pages, and
 * [ManagerScreen] provides its list recording to its own title bar. Page content sees null and
 * draws solid surfaces.
 *
 * Degradation: blur needs API 31 and the lens needs API 33 (the library skips both below that).
 * Below API 31, or without a hardware-accelerated window, glass falls back to [solidGlass]: the
 * tint composited over a raised surface with a hairline rim. On API 31-32 glass is blurred and
 * tinted without refraction.
 */

/** The recording behind the chrome drawn here; null in page content, previews and tests. */
internal val LocalGlassBackdrop = staticCompositionLocalOf<Backdrop?> { null }

@Composable
internal fun rememberGlassBackdrop(): LayerBackdrop {
    val page = Surfaces.page
    return rememberLayerBackdrop {
        drawRect(page)
        drawContent()
    }
}

/** Live glass needs RenderEffect (API 31) and a hardware-accelerated window. */
@Composable
@ReadOnlyComposable
internal fun glassLive(): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && LocalView.current.isHardwareAccelerated

/** Records this element for the chrome drawn above it; a no-op where glass is not live. */
@Composable
internal fun Modifier.glassSource(backdrop: LayerBackdrop): Modifier =
    if (glassLive()) this.layerBackdrop(backdrop) else this

/**
 * Glass surface: vibrancy, blur and edge refraction over [backdrop], with the specular rim and
 * soft shadow of iOS 27 chrome. [tint] is drawn over the refracted content; [blurRadius] grows for
 * panels that carry text.
 */
@Composable
internal fun Modifier.glassSurface(
    shape: Shape,
    tint: Color = Surfaces.glassTint,
    blurRadius: Dp = 4.dp,
    refraction: Dp = 16.dp,
    depth: Dp = 32.dp,
    backdrop: Backdrop? = LocalGlassBackdrop.current,
    fallbackElevation: Dp = 0.dp,
): Modifier {
    if (backdrop == null || !glassLive()) return solidGlass(shape, tint, fallbackElevation)
    return this.drawBackdrop(
        backdrop = backdrop,
        shape = { shape },
        effects = {
            vibrancy()
            blur(blurRadius.toPx())
            lens(
                refractionHeight = refraction.toPx(),
                refractionAmount = depth.toPx(),
                depthEffect = true,
            )
        },
        highlight = { Highlight.Default },
        shadow = { Shadow.Default },
        onDrawSurface = { drawRect(tint) },
    )
}

/** Opaque stand-in without a live backdrop: the tint over a raised surface plus a hairline rim. */
@Composable
internal fun Modifier.solidGlass(
    shape: Shape,
    tint: Color = Surfaces.glassTint,
    elevation: Dp = 0.dp,
): Modifier {
    val fill = tint.copy(alpha = .94f).compositeOver(Surfaces.raised)
    val rim = Surfaces.hairline
    return this
        .then(if (elevation > 0.dp) Modifier.shadow(elevation, shape, clip = false) else Modifier)
        .clip(shape)
        .background(fill)
        .border(0.5.dp, rim, shape)
}
