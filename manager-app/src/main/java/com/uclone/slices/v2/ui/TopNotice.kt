package com.uclone.slices.v2.ui

import android.os.SystemClock
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.animate
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The floating notice at the top of the Manager.
 *
 * - Each [PostedNotice] is timed on its own id, so a repeated notice restarts its timer and a timer
 *   that expires late can never clear a newer notice.
 * - The timer pauses while the card is touched or dragged.
 * - Swiping the card up (or tapping it) dismisses it at once.
 * - A first notice drops in from above the status bar; a notice that replaces another swaps in
 *   place instead of sliding past it.
 */
@Composable
internal fun TopNoticeHost(
    posted: PostedNotice?,
    onIntent: (UiIntent) -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduced = rememberReducedMotion()
    val density = LocalDensity.current
    // How far the card has to travel to start fully above the screen edge.
    val hiddenOffset = WindowInsets.statusBars.getTop(density) +
        with(density) { (Space.xs + NoticeShadow).roundToPx() }

    AnimatedContent(
        targetState = posted,
        contentKey = { it?.id },
        transitionSpec = {
            val entering = targetState
            val leaving = initialState
            when {
                reduced -> ContentTransform(
                    EnterTransition.None,
                    ExitTransition.None,
                    sizeTransform = null,
                )
                // Replacing one notice with another: swap in place, no crossing slides.
                entering != null && leaving != null -> ContentTransform(
                    targetContentEnter = fadeIn(Motion.effects()) +
                        scaleIn(Motion.spatial(), initialScale = .94f),
                    initialContentExit = fadeOut(Motion.exit()),
                    sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.size },
                )
                entering != null -> ContentTransform(
                    targetContentEnter = slideInVertically(Motion.offset) {
                        -(it + hiddenOffset)
                    } + fadeIn(Motion.effects()),
                    initialContentExit = ExitTransition.None,
                    sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.size },
                )
                else -> ContentTransform(
                    targetContentEnter = EnterTransition.None,
                    initialContentExit = slideOutVertically(Motion.exit()) {
                        -(it + hiddenOffset)
                    } + fadeOut(Motion.exit()),
                    // Keep the old height while the card leaves so it does not get squashed.
                    sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.exit() },
                )
            }
        },
        contentAlignment = Alignment.TopCenter,
        label = "top_notice",
        modifier = modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(start = Space.sm, top = Space.xs, end = Space.sm),
    ) { current ->
        if (current != null) {
            TopNoticeCard(
                posted = current,
                reduced = reduced,
                onDismiss = { onIntent(UiIntent.DismissNotice(current.id)) },
                onRetry = {
                    onIntent(UiIntent.DismissNotice(current.id))
                    onIntent(UiIntent.Refresh)
                },
            )
        }
    }
}

@Composable
private fun TopNoticeCard(
    posted: PostedNotice,
    reduced: Boolean,
    onDismiss: () -> Unit,
    onRetry: () -> Unit,
) {
    val notice = posted.notice
    val dismiss by rememberUpdatedState(onDismiss)
    var pressed by remember { mutableStateOf(false) }
    var dragging by remember { mutableStateOf(false) }

    LaunchedEffect(posted.id) {
        var remaining = noticeDurationMs(notice)
        while (remaining > 0) {
            snapshotFlow { pressed || dragging }.first { held -> !held }
            val started = SystemClock.uptimeMillis()
            val heldAgain = withTimeoutOrNull(remaining) {
                snapshotFlow { pressed || dragging }.first { held -> held }
            }
            if (heldAgain == null) break
            remaining -= SystemClock.uptimeMillis() - started
        }
        dismiss()
    }

    var dragOffset by remember { mutableFloatStateOf(0f) }
    val dismissDistance = with(LocalDensity.current) { NoticeSwipeDistance.toPx() }
    val dragState = rememberDraggableState { delta ->
        // Up follows the finger; down only gives a little, like a rubber band.
        dragOffset += if (dragOffset + delta > 0f) delta * .25f else delta
    }

    val success = isSuccessNotice(notice)
    val retry = isRetryNotice(notice)
    val tone = if (success) Tone.Success else Tone.Danger
    val headline = noticeHeadline(notice)
    val supporting = if (notice is UiNotice.SpaceActivated && notice.switched) {
        stringResource(R.string.success_switch_launch_hint)
    } else {
        null
    }
    val shape = RoundedCornerShape(NoticeRadius)
    Row(
        modifier = Modifier
            .widthIn(max = 560.dp)
            .fillMaxWidth()
            .graphicsLayer {
                translationY = dragOffset
                alpha = 1f - (-dragOffset / (dismissDistance * 2f)).coerceIn(0f, .6f)
            }
            // iOS 27 glass card refracting the page; solid glass with a shadow where glass is not live.
            .glassSurface(
                shape,
                tint = Surfaces.panelTint,
                blurRadius = 16.dp,
                refraction = 16.dp,
                depth = 28.dp,
                fallbackElevation = NoticeShadow,
            )
            .semantics { liveRegion = LiveRegionMode.Polite }
            .pointerInput(posted.id) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    val up = waitForUpOrCancellation()
                    pressed = false
                    // A plain tap dismisses; a drag is handled by draggable below.
                    if (up != null && !dragging && dragOffset == 0f) dismiss()
                }
            }
            .draggable(
                state = dragState,
                orientation = Orientation.Vertical,
                onDragStarted = { dragging = true },
                onDragStopped = { velocity ->
                    dragging = false
                    if (dragOffset < -dismissDistance || velocity < -NoticeFlingVelocity) {
                        dismiss()
                    } else if (reduced) {
                        dragOffset = 0f
                    } else {
                        animate(dragOffset, 0f, animationSpec = Motion.spatial()) { value, _ ->
                            dragOffset = value
                        }
                    }
                },
            )
            .padding(
                start = 10.dp,
                top = 10.dp,
                end = if (retry) Space.xs else 18.dp,
                bottom = 10.dp,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(32.dp)
                .clip(CircleShape)
                .background(Brush.linearGradient(tone.tileGradient())),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = if (success) "✓" else "!",
                color = Color.White,
                fontSize = TypeScale.headline,
                fontWeight = FontWeight.Bold,
            )
        }
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(start = Space.sm),
        ) {
            Text(
                text = headline,
                color = TextPrimary,
                fontSize = TypeScale.subhead,
                fontWeight = FontWeight.SemiBold,
                lineHeight = 20.sp,
            )
            supporting?.let {
                Text(
                    text = it,
                    color = TextSecondary,
                    fontSize = TypeScale.caption,
                    lineHeight = 17.sp,
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
        if (retry) {
            AppButton(
                text = stringResource(R.string.retry),
                onClick = onRetry,
                kind = ButtonKind.Text,
                compact = true,
            )
        }
    }
}

/** Glass card radius; the notice is about 52dp tall, so this reads as a capsule. */
private val NoticeRadius = Radius.card
private val NoticeShadow = 6.dp
private val NoticeSwipeDistance = 28.dp
private const val NoticeFlingVelocity = 900f

internal fun noticeDurationMs(notice: UiNotice): Long = when {
    isSuccessNotice(notice) -> 3_200L
    isRetryNotice(notice) -> 5_200L
    else -> 4_400L
}

internal fun isSuccessNotice(notice: UiNotice): Boolean =
    notice is UiNotice.RuntimeRestarted ||
        notice is UiNotice.ConfigurationCompleted ||
        notice is UiNotice.ConfigurationRebound ||
        notice is UiNotice.SpaceCreated ||
        notice is UiNotice.SpaceActivated ||
        notice is UiNotice.SpaceRenamed ||
        notice is UiNotice.SpaceDeleted ||
        notice is UiNotice.AppUnenrolled

internal fun isRetryNotice(notice: UiNotice): Boolean =
    notice is UiNotice.LocalAppsUnavailable ||
        notice is UiNotice.RuntimeUnavailable ||
        notice is UiNotice.MissingEntity ||
        notice is UiNotice.StateChanged ||
        notice is UiNotice.OperationFailed

@Composable
private fun noticeHeadline(notice: UiNotice): String =
    if (notice is UiNotice.SpaceActivated && notice.switched) {
        stringResource(R.string.success_switched_title, notice.name)
    } else {
        noticeMessage(notice)
    }

@Composable
private fun noticeMessage(notice: UiNotice): String = when (notice) {
    UiNotice.LocalAppsUnavailable -> stringResource(R.string.error_local_apps)
    UiNotice.RuntimeUnavailable -> stringResource(R.string.error_runtime_unavailable)
    UiNotice.InvalidInput -> stringResource(R.string.error_invalid_input)
    UiNotice.MissingEntity -> stringResource(R.string.error_missing_entity)
    UiNotice.StateChanged -> stringResource(R.string.error_state_changed)
    UiNotice.OperationFailed -> stringResource(R.string.error_operation_failed)
    UiNotice.PackageUnavailable -> stringResource(R.string.package_unavailable_description)
    UiNotice.IdentityProtected -> stringResource(R.string.error_identity_protected)
    UiNotice.SigningUnavailable -> stringResource(R.string.error_signing_unavailable)
    UiNotice.RuntimeVersionMismatch -> stringResource(R.string.error_runtime_version_mismatch)
    UiNotice.AccountIoBusy -> stringResource(R.string.error_account_io_busy)
    UiNotice.StorageInsufficient -> stringResource(R.string.error_storage_insufficient)
    UiNotice.RuntimeRestartFailed -> stringResource(R.string.error_runtime_restart_failed)
    UiNotice.RuntimeRestarted -> stringResource(R.string.success_runtime_restarted)
    UiNotice.ConfigurationCompleted -> stringResource(R.string.success_configured)
    UiNotice.ConfigurationRebound -> stringResource(R.string.success_rebound)
    is UiNotice.SpaceCreated -> stringResource(R.string.success_created, notice.name)
    is UiNotice.SpaceActivated -> if (notice.switched) {
        stringResource(R.string.success_switched, notice.name)
    } else {
        stringResource(R.string.success_launched, notice.name)
    }
    is UiNotice.SpaceRenamed -> stringResource(R.string.success_renamed, notice.name)
    is UiNotice.SpaceDeleted -> stringResource(R.string.success_deleted, notice.name)
    is UiNotice.AppUnenrolled ->
        stringResource(R.string.success_unenrolled, notice.appLabel)
}
