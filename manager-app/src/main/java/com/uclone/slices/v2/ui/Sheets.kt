package com.uclone.slices.v2.ui

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.launch

/** Keeps the last non-null [value] so a sheet can finish its exit after the state clears. */
@Composable
internal fun <T : Any> rememberRetained(value: T?): T? {
    val holder = remember { RetainedHolder<T>() }
    if (value != null) holder.value = value
    return holder.value
}

private class RetainedHolder<T : Any> {
    var value: T? = null
}

/**
 * Bottom sheet used for every confirmation, choice and form: an iOS 27 floating Liquid Glass panel
 * inset 12dp from the screen edges, radius 34, 36x5 drag handle, drag-down or scrim tap to
 * dismiss, predictive back. Enters with spring(0.9, 380); exits in 220ms on the accelerating
 * curve. It stays composed while the exit runs; callbacks only fire while it is visible.
 * [dismissible] = false keeps it open against back, drag and scrim. The panel refracts
 * [LocalGlassBackdrop]; buttons inside it are glass capsules.
 */
@Composable
internal fun BottomSheet(
    visible: Boolean,
    onDismiss: () -> Unit,
    title: String?,
    summary: String? = null,
    dismissible: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    val reduced = rememberReducedMotion()
    val hidden = remember { Animatable(1f) }
    var present by remember { mutableStateOf(visible) }
    var height by remember { mutableIntStateOf(0) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(visible) {
        if (visible) {
            present = true
            if (reduced) hidden.snapTo(0f) else hidden.animateTo(0f, Motion.sheetEnter())
        } else {
            if (reduced) hidden.snapTo(1f) else hidden.animateTo(1f, Motion.exit())
            present = false
        }
    }
    PredictiveBackHandler(enabled = visible && present) { progress ->
        try {
            progress.collect { if (dismissible) hidden.snapTo(it.progress * .2f) }
            if (dismissible) {
                onDismiss()
            } else {
                hidden.animateTo(0f, Motion.sheetEnter())
            }
        } catch (cancelled: CancellationException) {
            scope.launch { hidden.animateTo(0f, Motion.sheetEnter()) }
            throw cancelled
        }
    }
    if (!present) return
    val scrim = Surfaces.scrim
    val scrimAlpha by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = if (reduced) snap() else tween(Motion.ScrimFadeMs),
        label = "sheet-scrim",
    )
    val drag = rememberDraggableState { delta ->
        if (height > 0) scope.launch { hidden.snapTo((hidden.value + delta / height).coerceIn(0f, 1f)) }
    }
    val inset = with(LocalDensity.current) { Space.panelInset.toPx() }
    val sheetShape = RoundedCornerShape(Radius.sheet)
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier
                .fillMaxSize()
                .graphicsLayer { alpha = scrimAlpha * (1f - hidden.value * .6f) }
                .background(scrim)
                .clickable(
                    interactionSource = null,
                    indication = null,
                    onClickLabel = stringResource(R.string.cancel),
                ) { if (visible && dismissible) onDismiss() },
        )
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .windowInsetsPadding(
                    WindowInsets.ime
                        .union(WindowInsets.navigationBars)
                        .union(WindowInsets.safeDrawing.only(WindowInsetsSides.Top)),
                )
                .padding(all = Space.panelInset)
                .widthIn(max = 640.dp)
                .fillMaxWidth()
                .onSizeChanged { height = it.height }
                .graphicsLayer {
                    translationY = hidden.value * (if (height > 0) height + inset * 3f else 2000f)
                }
                .glassSurface(
                    sheetShape,
                    tint = Surfaces.panelTint,
                    blurRadius = 24.dp,
                    refraction = 24.dp,
                    depth = 40.dp,
                    fallbackElevation = 16.dp,
                )
                .draggable(
                    state = drag,
                    orientation = Orientation.Vertical,
                    enabled = visible && dismissible,
                    onDragStopped = { velocity ->
                        if (hidden.value > .25f || velocity > 1800f) {
                            onDismiss()
                        } else {
                            scope.launch { hidden.animateTo(0f, Motion.sheetEnter()) }
                        }
                    },
                )
                .padding(start = Space.md, end = Space.md, bottom = Space.md),
        ) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .padding(top = 8.dp, bottom = Space.sm),
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier
                        .size(width = 36.dp, height = 5.dp)
                        .clip(CircleShape)
                        .background(TextTertiary),
                )
            }
            Column(
                Modifier
                    .weight(1f, fill = false)
                    .verticalScroll(rememberScrollState()),
            ) {
                if (title != null) {
                    Text(
                        text = title,
                        color = TextPrimary,
                        fontSize = TypeScale.title,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.padding(horizontal = Space.xxs),
                    )
                }
                if (!summary.isNullOrBlank()) {
                    Text(
                        text = summary,
                        color = TextSecondary,
                        fontSize = TypeScale.subhead,
                        lineHeight = 20.sp,
                        modifier = Modifier.padding(start = Space.xxs, end = Space.xxs, top = Space.xs),
                    )
                }
                Spacer(Modifier.height(Space.md))
                content()
            }
        }
    }
}

/**
 * Centred iOS 27 glass alert, reserved for the short blocking "operation in progress" state:
 * scale 0.94 -> 1 plus fade on spring(0.9, 500); exits on the 220ms accelerating curve. Not
 * dismissible. Any buttons placed in it are glass capsules.
 */
@Composable
internal fun CenterDialog(
    visible: Boolean,
    content: @Composable ColumnScope.() -> Unit,
) {
    val reduced = rememberReducedMotion()
    val shown = remember { Animatable(0f) }
    var present by remember { mutableStateOf(visible) }
    LaunchedEffect(visible) {
        if (visible) {
            present = true
            if (reduced) shown.snapTo(1f) else shown.animateTo(1f, Motion.dialogEnter())
        } else {
            if (reduced) shown.snapTo(0f) else shown.animateTo(0f, Motion.exit())
            present = false
        }
    }
    // Swallow back while the operation runs, as the previous non-dismissible dialog did.
    PredictiveBackHandler(enabled = visible) { progress -> progress.collect { } }
    if (!present) return
    val scrim = Surfaces.scrim
    Box(
        Modifier
            .fillMaxSize()
            .clickable(interactionSource = null, indication = null) { },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .graphicsLayer { alpha = shown.value.coerceIn(0f, 1f) }
                .background(scrim),
        )
        Column(
            modifier = Modifier
                .padding(horizontal = Space.xl)
                .widthIn(max = 340.dp)
                .fillMaxWidth()
                .graphicsLayer {
                    val s = Motion.DialogInitialScale + (1f - Motion.DialogInitialScale) * shown.value
                    scaleX = s
                    scaleY = s
                    alpha = shown.value.coerceIn(0f, 1f)
                }
                .glassSurface(
                    RoundedCornerShape(Radius.sheet),
                    tint = Surfaces.panelTint,
                    blurRadius = 24.dp,
                    refraction = 24.dp,
                    depth = 40.dp,
                    fallbackElevation = 16.dp,
                )
                .padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
            content = content,
        )
    }
}
