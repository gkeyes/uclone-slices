package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.uclone.slices.v2.ui.liquid.LiquidButton
import kotlin.math.roundToInt

internal enum class ButtonKind { Primary, Secondary, Destructive, Text }

/**
 * Capsule buttons: primary filled, secondary tonal (accent 12%), destructive (danger), text. Drawn
 * over glass chrome (sheets, alerts) they are Liquid Glass buttons refracting the page, tinted by
 * kind; on page content they are solid capsules. All get press feedback; primary and destructive
 * confirm with a haptic, text buttons that cancel a dangerous action can opt into the reject
 * haptic.
 */
@Composable
internal fun AppButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.Primary,
    enabled: Boolean = true,
    loading: Boolean = false,
    icon: Painter? = null,
    compact: Boolean = false,
    rejectHaptic: Boolean = false,
) {
    val haptic = LocalHapticFeedback.current
    val live = glassLive()
    val backdrop = LocalGlassBackdrop.current
    val glassBackdrop = if (live && kind != ButtonKind.Text) backdrop else null
    val shape = RoundedCornerShape(Radius.button)
    val targetContainer = when (kind) {
        ButtonKind.Primary -> Accent
        ButtonKind.Secondary -> Accent.copy(alpha = .12f)
        ButtonKind.Destructive -> Tone.Danger.container()
        ButtonKind.Text -> Color.Transparent
    }
    val targetContent = when (kind) {
        ButtonKind.Primary -> OnAccent
        ButtonKind.Secondary, ButtonKind.Text -> Accent
        ButtonKind.Destructive -> if (glassBackdrop != null) Color.White else Tone.Danger.onContainer()
    }
    val container by animateColorAsState(targetContainer, Motion.effects(), label = "button-bg")
    val content by animateColorAsState(targetContent, Motion.effects(), label = "button-fg")
    val fade by animateFloatAsState(if (enabled) 1f else TextAlpha.Disabled, Motion.effects(), label = "button-alpha")
    val click: () -> Unit = {
        when {
            kind == ButtonKind.Primary || kind == ButtonKind.Destructive ->
                haptic.performHapticFeedback(HapticFeedbackType.Confirm)
            rejectHaptic -> haptic.performHapticFeedback(HapticFeedbackType.Reject)
        }
        onClick()
    }
    val label: @Composable RowScope.() -> Unit = {
        if (loading || icon != null) {
            AnimatedContent(
                targetState = loading,
                transitionSpec = fadeThrough(),
                label = "button-leading",
            ) { busy ->
                if (busy) {
                    SlicesSpinner(size = 18.dp, color = content)
                } else if (icon != null) {
                    Icon(painter = icon, contentDescription = null, tint = content, modifier = Modifier.size(20.dp))
                }
            }
        }
        Text(
            text = text,
            color = content,
            fontSize = if (compact) TypeScale.subhead else TypeScale.body,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
    if (glassBackdrop != null) {
        LiquidButton(
            onClick = click,
            backdrop = glassBackdrop,
            modifier = modifier.graphicsLayer { alpha = fade },
            enabled = enabled && !loading,
            tint = when (kind) {
                ButtonKind.Primary -> Accent
                ButtonKind.Destructive -> SystemRed
                ButtonKind.Secondary, ButtonKind.Text -> Color.Unspecified
            },
            surfaceColor = if (kind == ButtonKind.Secondary) Surfaces.glassTint else Color.Unspecified,
            buttonHeight = if (compact) CompactButtonHeight else ButtonHeight,
            horizontalPadding = if (compact) Space.sm else Space.lg,
            content = label,
        )
        return
    }
    Box(
        modifier = modifier
            .pressFeedback(enabled && !loading, Motion.PressedScale, shape)
            .graphicsLayer { alpha = fade }
            .heightIn(min = if (compact) CompactButtonHeight else ButtonHeight)
            .clip(shape)
            .background(container)
            .clickable(
                enabled = enabled && !loading,
                role = Role.Button,
                interactionSource = null,
                indication = null,
                onClick = click,
            )
            .padding(
                horizontal = if (compact) Space.sm else Space.lg,
                vertical = if (compact) 6.dp else Space.sm,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.xs),
            content = label,
        )
    }
}

private val ButtonHeight = 50.dp
private val CompactButtonHeight = 36.dp

/** Cancel + confirm capsule pair used at the bottom of every confirmation sheet. */
@Composable
internal fun SheetActions(
    cancelText: String,
    confirmText: String,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    confirmEnabled: Boolean = true,
    destructive: Boolean = false,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = Space.lg),
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        AppButton(
            text = cancelText,
            onClick = onCancel,
            kind = ButtonKind.Secondary,
            rejectHaptic = destructive,
            modifier = Modifier.weight(1f),
        )
        AppButton(
            text = confirmText,
            onClick = onConfirm,
            kind = if (destructive) ButtonKind.Destructive else ButtonKind.Primary,
            enabled = confirmEnabled,
            modifier = Modifier.weight(1f),
        )
    }
}

// ---------- Segmented ----------

/**
 * iOS segmented control: a capsule track whose raised capsule indicator slides on the spatial
 * spring; a tick on change.
 */
@Composable
internal fun Segmented(
    options: List<String>,
    selected: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    enabled: (Int) -> Boolean = { true },
) {
    val haptic = LocalHapticFeedback.current
    val reduced = rememberReducedMotion()
    val shape = RoundedCornerShape(Radius.control)
    val indicator = if (isSystemInDarkTheme()) Color(0xFF636366) else Color.White
    BoxWithConstraints(
        modifier
            .fillMaxWidth()
            .height(40.dp)
            .clip(shape)
            .background(Surfaces.control)
            .padding(3.dp),
    ) {
        val count = options.size.coerceAtLeast(1)
        val segment = maxWidth / count
        val segmentPx = with(LocalDensity.current) { segment.toPx() }
        val position = remember { Animatable(selected.toFloat()) }
        LaunchedEffect(selected, reduced) {
            if (reduced) position.snapTo(selected.toFloat()) else position.animateTo(selected.toFloat(), Motion.spatial())
        }
        val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
        Box(
            Modifier
                .width(segment)
                .fillMaxHeight()
                .offset { IntOffset((position.value * segmentPx * if (rtl) -1f else 1f).roundToInt(), 0) }
                .shadow(3.dp, shape)
                .background(indicator, shape),
        )
        Row(Modifier.fillMaxSize()) {
            options.forEachIndexed { index, label ->
                val optionEnabled = enabled(index)
                val color by animateColorAsState(
                    when {
                        !optionEnabled -> TextDisabled
                        index == selected -> TextPrimary
                        else -> TextSecondary
                    },
                    Motion.effects(),
                    label = "segment",
                )
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .clickable(
                            enabled = optionEnabled,
                            role = Role.Tab,
                            interactionSource = null,
                            indication = null,
                        ) {
                            if (index != selected) {
                                haptic.performHapticFeedback(HapticFeedbackType.SegmentTick)
                                onSelect(index)
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = label,
                        color = color,
                        fontSize = TypeScale.subhead,
                        fontWeight = if (index == selected) FontWeight.SemiBold else FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(horizontal = Space.xs),
                    )
                }
            }
        }
    }
}
