package com.uclone.slices.v2.ui

import android.provider.Settings
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import kotlinx.coroutines.delay

/**
 * True when the user turned animations off (animator duration scale 0). Decorative entrances are
 * skipped and custom animatables snap. No logic waits for an animation to finish.
 */
@Composable
internal fun rememberReducedMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f) == 0f
    }
}

/**
 * Press feedback per spec: scale to [pressedScale] (0.965 for cards/buttons, 0.985 for full-width
 * rows) with spring(1.0, 1500) and a 6% state layer clipped to [shape]; release springs back with
 * spring(0.62, 420). Observes the Initial pointer pass so the wrapped element keeps its own click
 * handling and semantics, and cancels once the touch turns into a scroll.
 */
@Composable
internal fun Modifier.pressFeedback(
    enabled: Boolean = true,
    pressedScale: Float = Motion.PressedScale,
    shape: Shape = RoundedCornerShape(Radius.row),
): Modifier {
    var pressed by remember { mutableStateOf(false) }
    val active = pressed && enabled
    val reduced = rememberReducedMotion()
    val scale by animateFloatAsState(
        targetValue = if (active) pressedScale else 1f,
        animationSpec = when {
            reduced -> snap()
            active -> Motion.pressIn()
            else -> Motion.pressRelease()
        },
        label = "press-scale",
    )
    val layer by animateFloatAsState(
        targetValue = if (active) 1f else 0f,
        animationSpec = when {
            reduced -> snap()
            active -> Motion.pressIn()
            else -> Motion.effects()
        },
        label = "press-layer",
    )
    val stateLayer = Surfaces.stateLayer
    val gesture = if (!enabled) {
        Modifier
    } else {
        Modifier.pointerInput(Unit) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                pressed = true
                val slop = viewConfiguration.touchSlop
                try {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed || (change.position - down.position).getDistance() > slop) break
                    }
                } finally {
                    pressed = false
                }
            }
        }
    }
    return this
        .graphicsLayer {
            scaleX = scale
            scaleY = scale
        }
        .drawWithContent {
            drawContent()
            if (layer > 0f) {
                drawOutline(
                    shape.createOutline(size, layoutDirection, this),
                    stateLayer,
                    alpha = layer.coerceIn(0f, 1f),
                )
            }
        }
        .then(gesture)
}

/** Screens whose first-entrance stagger already played in this process. */
private val playedStaggers = mutableSetOf<String>()

/**
 * True during the first entrance of [screen] in this process. It turns false shortly after, so
 * items that scroll into view later (or the screen on a later visit) do not replay it.
 */
@Composable
internal fun rememberStaggerPlay(screen: String): Boolean {
    val first = remember(screen) { playedStaggers.add(screen) }
    var active by remember(screen) { mutableStateOf(first) }
    if (active) {
        LaunchedEffect(screen) {
            delay(StaggerWindowMs)
            active = false
        }
    }
    return active
}

private const val StaggerWindowMs = 600L

/**
 * First-entrance stagger: the first [Motion.StaggerMaxItems] blocks rise 16dp and fade in, 26ms
 * apart, on the spatial spring. Later items and reduced-motion users get the final state.
 */
@Composable
internal fun Modifier.staggerIn(index: Int, play: Boolean): Modifier {
    val reduced = rememberReducedMotion()
    val animate = remember { play && !reduced && index < Motion.StaggerMaxItems }
    val progress = remember { Animatable(if (animate) 0f else 1f) }
    if (animate) {
        LaunchedEffect(Unit) {
            delay(index * Motion.StaggerMs)
            progress.animateTo(1f, Motion.spatial())
        }
    }
    val rise = with(LocalDensity.current) { Motion.StaggerOffset.toPx() }
    return this.graphicsLayer {
        val p = progress.value
        alpha = p.coerceIn(0f, 1f)
        translationY = (1f - p) * rise
    }
}

/** Fade-through for wording that changes with state (chips, titles, summaries). */
internal fun <S> fadeThrough(): AnimatedContentTransitionScope<S>.() -> ContentTransform = {
    ContentTransform(
        targetContentEnter = fadeIn(Motion.effects()) + slideInVertically(Motion.offset) { it / 6 },
        initialContentExit = fadeOut(Motion.effects()),
        sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.size },
    )
}

/** Vertical roll for changing numbers: up when the value grows, down when it shrinks. */
internal fun rollingNumber(): AnimatedContentTransitionScope<String>.() -> ContentTransform = {
    val up = (targetState.digitsValue() ?: 0L) >= (initialState.digitsValue() ?: 0L)
    val direction = if (up) 1 else -1
    ContentTransform(
        targetContentEnter = slideInVertically(Motion.offset) { it / 2 * direction } + fadeIn(Motion.effects()),
        initialContentExit = androidx.compose.animation.slideOutVertically(Motion.offset) { -it / 2 * direction } +
            fadeOut(Motion.effects()),
        sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.size },
    )
}

private fun String.digitsValue(): Long? = filter(Char::isDigit).take(15).toLongOrNull()

/**
 * Shared X-axis page transition: 30dp travel + fade on spring(1.0, 380); [forward] decides the
 * direction, back reverses it. Reduced motion swaps pages without movement.
 */
internal fun sharedAxisX(forward: Boolean, offsetPx: Int, reduced: Boolean): ContentTransform {
    if (reduced) {
        return ContentTransform(
            targetContentEnter = EnterTransition.None,
            initialContentExit = ExitTransition.None,
            sizeTransform = null,
        )
    }
    val direction = if (forward) 1 else -1
    return (
        slideInHorizontally(Motion.pageOffset) { offsetPx * direction } +
            fadeIn(Motion.page())
        ) togetherWith (
        slideOutHorizontally(Motion.pageOffset) { -offsetPx * direction } +
            fadeOut(Motion.page())
        )
}
