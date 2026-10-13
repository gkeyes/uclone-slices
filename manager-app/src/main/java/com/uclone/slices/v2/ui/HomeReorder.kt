package com.uclone.slices.v2.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** Lazy item key of a row in reorder mode. */
internal fun reorderItemKey(packageName: String): String = REORDER_KEY_PREFIX + packageName

private const val REORDER_KEY_PREFIX = "reorder:"

/**
 * Drag state of the home reorder list. The dragged row follows the finger through [offset];
 * when its centre crosses a neighbour, [onMove] swaps them in the draft and the offset is
 * rebased on the new slot, so neighbours glide out of the way through their item placement
 * animation. On release the residual offset springs back to 0 (snaps with reduced motion).
 */
@Stable
internal class HomeReorderState(
    private val listState: LazyListState,
    private val scope: CoroutineScope,
    private val reduced: Boolean,
    private val onMove: (from: String, to: String) -> Unit,
    private val onCross: () -> Unit,
) {
    var draggingKey by mutableStateOf<String?>(null)
        private set
    private var dragOffset by mutableFloatStateOf(0f)
    var settlingKey by mutableStateOf<String?>(null)
        private set
    private val settle = Animatable(0f)

    /** Current translation of [packageName]'s row. */
    fun offsetOf(packageName: String): Float = when (packageName) {
        draggingKey -> dragOffset
        settlingKey -> settle.value
        else -> 0f
    }

    fun start(packageName: String) {
        draggingKey = packageName
        dragOffset = 0f
    }

    fun drag(delta: Float) {
        if (draggingKey == null) return
        dragOffset += delta
        swapIfNeeded()
    }

    fun end() {
        val key = draggingKey ?: return
        val residual = dragOffset
        draggingKey = null
        dragOffset = 0f
        if (reduced || residual == 0f) return
        settlingKey = key
        scope.launch {
            settle.snapTo(residual)
            settle.animateTo(0f, Motion.spatial())
            if (settlingKey == key) settlingKey = null
        }
    }

    /** One auto-scroll step near the viewport edges; returns false when no drag is active. */
    suspend fun autoScroll(edgePx: Float, maxStepPx: Float): Boolean {
        val key = draggingKey ?: return false
        val layout = listState.layoutInfo
        val item = layout.visibleItemsInfo.firstOrNull { it.key == reorderItemKey(key) } ?: return true
        val top = item.offset + dragOffset
        val bottom = top + item.size
        val step = when {
            top < layout.viewportStartOffset + edgePx ->
                -maxStepPx * ((layout.viewportStartOffset + edgePx - top) / edgePx).coerceIn(0f, 1f)
            bottom > layout.viewportEndOffset - edgePx ->
                maxStepPx * ((bottom - (layout.viewportEndOffset - edgePx)) / edgePx).coerceIn(0f, 1f)
            else -> 0f
        }
        if (step != 0f) {
            val consumed = listState.scrollBy(step)
            if (consumed != 0f && draggingKey == key) {
                dragOffset += consumed
                swapIfNeeded()
            }
        }
        return true
    }

    private fun swapIfNeeded() {
        val key = draggingKey ?: return
        val items = listState.layoutInfo.visibleItemsInfo
        val current = items.firstOrNull { it.key == reorderItemKey(key) } ?: return
        val centre = current.offset + dragOffset + current.size / 2f
        val target = items.firstOrNull { info ->
            val other = info.key as? String
            other != null &&
                other.startsWith(REORDER_KEY_PREFIX) &&
                info.key != current.key &&
                centre >= info.offset &&
                centre <= info.offset + info.size
        } ?: return
        // Keep the scroll anchor: Lazy lists follow the first visible item when it moves.
        val firstIndex = listState.firstVisibleItemIndex
        if (firstIndex == current.index || firstIndex == target.index) {
            listState.requestScrollToItem(firstIndex, listState.firstVisibleItemScrollOffset)
        }
        onMove(key, (target.key as String).removePrefix(REORDER_KEY_PREFIX))
        dragOffset += current.offset - target.offset
        onCross()
    }
}

@Composable
internal fun rememberHomeReorderState(
    listState: LazyListState,
    onMove: (from: String, to: String) -> Unit,
): HomeReorderState {
    val scope = rememberCoroutineScope()
    val reduced = rememberReducedMotion()
    val haptic = LocalHapticFeedback.current
    val currentOnMove by rememberUpdatedState(onMove)
    val state = remember(listState, reduced) {
        HomeReorderState(
            listState = listState,
            scope = scope,
            reduced = reduced,
            onMove = { from, to -> currentOnMove(from, to) },
            onCross = { haptic.performHapticFeedback(HapticFeedbackType.SegmentTick) },
        )
    }
    val density = LocalDensity.current
    val edge = with(density) { 72.dp.toPx() }
    val step = with(density) { 14.dp.toPx() }
    LaunchedEffect(state.draggingKey) {
        if (state.draggingKey == null) return@LaunchedEffect
        while (isActive) {
            withFrameNanos { }
            if (!state.autoScroll(edge, step)) break
        }
    }
    return state
}

/**
 * Compact row of the reorder list: icon, name, account count, overflow with move up/down and a
 * drag handle. Dragging lifts the row (scale + shadow) above its neighbours.
 */
@Composable
internal fun ReorderRow(
    app: InstalledApp,
    accountCount: Int,
    index: Int,
    lastIndex: Int,
    state: HomeReorderState,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val haptic = LocalHapticFeedback.current
    val reduced = rememberReducedMotion()
    val dragging = state.draggingKey == app.packageName
    val lifted = dragging || state.settlingKey == app.packageName
    val lift by animateFloatAsState(
        targetValue = if (dragging) 1f else 0f,
        animationSpec = if (reduced) snap() else Motion.spatial(),
        label = "reorder-lift",
    )
    val moveUpText = stringResource(R.string.move_up)
    val moveDownText = stringResource(R.string.move_down)
    val handleText = stringResource(R.string.drag_to_reorder)
    val shape = RoundedCornerShape(Radius.row)
    val packageName = app.packageName
    val dragCallbacks: suspend PointerInputScope.(Boolean) -> Unit =
        { afterLongPress ->
            val onStart: (Offset) -> Unit = {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                state.start(packageName)
            }
            val onEnd: () -> Unit = {
                if (state.draggingKey == packageName) {
                    haptic.performHapticFeedback(HapticFeedbackType.Confirm)
                }
                state.end()
            }
            if (afterLongPress) {
                detectDragGesturesAfterLongPress(
                    onDragStart = onStart,
                    onDragEnd = onEnd,
                    onDragCancel = { state.end() },
                    onDrag = { change, amount ->
                        change.consume()
                        state.drag(amount.y)
                    },
                )
            } else {
                detectDragGestures(
                    onDragStart = onStart,
                    onDragEnd = onEnd,
                    onDragCancel = { state.end() },
                    onDrag = { change, amount ->
                        change.consume()
                        state.drag(amount.y)
                    },
                )
            }
        }
    Row(
        modifier = modifier
            .zIndex(if (lifted) 1f else 0f)
            .graphicsLayer {
                translationY = state.offsetOf(packageName)
                val scale = 1f + 0.02f * lift
                scaleX = scale
                scaleY = scale
                shadowElevation = 8.dp.toPx() * lift
                this.shape = shape
                clip = false
            }
            .fillMaxWidth()
            .padding(horizontal = Space.gutter)
            .clip(shape)
            .background(if (lifted) Surfaces.raised else Surfaces.card)
            .pointerInput(packageName) { dragCallbacks(this, true) }
            .semantics(mergeDescendants = true) {
                customActions = buildList {
                    if (index > 0) add(CustomAccessibilityAction(moveUpText) { onMoveUp(); true })
                    if (index < lastIndex) add(CustomAccessibilityAction(moveDownText) { onMoveDown(); true })
                }
            }
            .heightIn(min = Space.rowMinHeight)
            .padding(start = Space.md, end = Space.xxs, top = Space.xs, bottom = Space.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        AppIcon(app.label, app.icon, 36.dp)
        Column(Modifier.weight(1f)) {
            Text(
                text = app.label,
                color = TextPrimary,
                fontSize = TypeScale.body,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = stringResource(R.string.independent_space_count, accountCount),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                style = Tabular,
                maxLines = 1,
            )
        }
        SlicesOverflowMenu(
            items = buildList {
                if (index > 0) add(SlicesMenuItem(text = moveUpText, onClick = onMoveUp))
                if (index < lastIndex) add(SlicesMenuItem(text = moveDownText, onClick = onMoveDown))
            },
            contentDescription = stringResource(R.string.reorder_actions),
            enabled = state.draggingKey == null,
        )
        Box(
            modifier = Modifier
                .size(Space.touchTarget)
                .pointerInput(packageName) { dragCallbacks(this, false) }
                .semantics { contentDescription = handleText },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_drag_handle),
                contentDescription = null,
                tint = if (dragging) Accent else TextTertiary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}
