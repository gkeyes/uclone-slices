package com.uclone.slices.v2.ui

import android.graphics.drawable.Drawable
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.graphics.drawable.toBitmap
import com.kyant.backdrop.backdrops.layerBackdrop
import com.kyant.backdrop.backdrops.rememberLayerBackdrop
import com.kyant.shapes.Capsule
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.liquid.LiquidToggle

/** Section header above a card, aligned with the text inside the card (gutter + 16). */
@Composable
internal fun SectionHeader(
    text: String,
    modifier: Modifier = Modifier,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(
                start = Space.textInset,
                end = if (trailing == null) Space.textInset else Space.gutter,
                top = Space.sm,
            )
            .heightIn(min = 28.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = text,
            color = TextSecondary,
            fontSize = TypeScale.sectionHeader,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke(this)
    }
}

/** Secondary note under a card, aligned with card text. */
@Composable
internal fun Footnote(text: String, modifier: Modifier = Modifier, color: Color = TextSecondary) {
    Text(
        text = text,
        color = color,
        fontSize = TypeScale.caption,
        lineHeight = 18.sp,
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = Space.textInset),
    )
}

/**
 * Grouped card (radius 26): no dividers; rows are separated by their 52dp height and insets. Placed on the
 * page gutter; size changes use the spatial spring.
 */
@Composable
internal fun GroupCard(
    modifier: Modifier = Modifier,
    padded: Boolean = false,
    onClick: (() -> Unit)? = null,
    enabled: Boolean = true,
    color: Color = Surfaces.card,
    content: @Composable ColumnScope.() -> Unit,
) {
    SlicesPanel(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = Space.gutter),
        cornerRadius = Radius.card,
        insideMargin = if (padded) PaddingValues(Space.cardPadding) else PaddingValues(vertical = Space.xxs),
        enabled = enabled,
        onClick = onClick,
        color = color,
        content = content,
    )
}

/**
 * One row of a grouped card split across lazy items (long lists): only the first and last rows
 * carry the outer corners. Use with a screen whose item spacing is 0.
 */
@Composable
internal fun GroupedItem(
    first: Boolean,
    last: Boolean,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val top = if (first) Radius.card else 0.dp
    val bottom = if (last) Radius.card else 0.dp
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = Space.gutter)
            .clip(RoundedCornerShape(topStart = top, topEnd = top, bottomStart = bottom, bottomEnd = bottom))
            .background(Surfaces.card)
            .padding(top = if (first) Space.xxs else 0.dp, bottom = if (last) Space.xxs else 0.dp),
        content = content,
    )
}

/** Hairline that starts where row text starts; the only allowed separator inside a card. */
@Composable
internal fun InsetHairline(start: Dp = Space.md) {
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = start)
            .height(0.5.dp)
            .background(Surfaces.hairline),
    )
}

/**
 * Leading icon: an iOS Settings-style tile (30dp, radius 8) with a white glyph on the tone's
 * gradient; the gradient cross-fades when the tone changes.
 */
@Composable
internal fun IconPlate(painter: Painter, tone: Tone = Tone.Info, size: Dp = 30.dp) {
    val start by animateColorAsState(tone.tileGradient()[0], Motion.effects(), label = "plate-start")
    val end by animateColorAsState(tone.tileGradient()[1], Motion.effects(), label = "plate-end")
    Box(
        modifier = Modifier
            .size(size)
            .clip(RoundedCornerShape(Radius.icon * (size / 30.dp)))
            .background(Brush.linearGradient(listOf(start, end))),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painter = painter, contentDescription = null, tint = Color.White, modifier = Modifier.size(size * 0.6f))
    }
}

/**
 * Row: leading (icon plate or custom) + title / subtitle + trailing (value, custom, chevron). The
 * whole row is the touch target and gets row press feedback (0.985 + state layer).
 */
@Composable
internal fun ListRow(
    title: String,
    modifier: Modifier = Modifier,
    summary: String? = null,
    value: String? = null,
    valueColor: Color = TextSecondary,
    icon: Painter? = null,
    iconTone: Tone = Tone.Info,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
    enabled: Boolean = true,
    chevron: Boolean = onClick != null,
    titleMaxLines: Int = 1,
    summaryMaxLines: Int = 3,
    monospaceValue: Boolean = false,
) {
    val interactive = onClick != null
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (interactive) {
                    Modifier
                        .pressFeedback(enabled, Motion.PressedScaleRow, RoundedCornerShape(Radius.row))
                        .clickable(
                            enabled = enabled,
                            role = Role.Button,
                            interactionSource = null,
                            indication = null,
                        ) { onClick?.invoke() }
                } else {
                    Modifier
                },
            )
            .graphicsLayer { alpha = if (enabled) 1f else TextAlpha.Disabled / TextAlpha.Primary }
            .heightIn(min = Space.rowMinHeight)
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        if (leading != null) leading() else if (icon != null) IconPlate(icon, iconTone)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                text = title,
                color = TextPrimary,
                fontSize = TypeScale.body,
                lineHeight = 21.sp,
                maxLines = titleMaxLines,
                overflow = TextOverflow.Ellipsis,
            )
            if (!summary.isNullOrBlank()) {
                Text(
                    text = summary,
                    color = TextSecondary,
                    fontSize = TypeScale.subhead,
                    lineHeight = 19.sp,
                    maxLines = summaryMaxLines,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (!value.isNullOrEmpty()) {
            AnimatedContent(
                targetState = value,
                transitionSpec = rollingNumber(),
                contentAlignment = Alignment.CenterEnd,
                label = "row-value",
                modifier = Modifier.widthIn(max = 180.dp),
            ) { shown ->
                Text(
                    text = shown,
                    color = valueColor,
                    fontSize = TypeScale.subhead,
                    fontFamily = if (monospaceValue) FontFamily.Monospace else null,
                    style = Tabular,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.End,
                )
            }
        }
        trailing?.invoke(this)
        if (chevron) {
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                tint = TextTertiary,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

/**
 * Switch row: the whole row toggles and emits the toggleOn / toggleOff haptic exactly once. The
 * switch is the iOS 27 [GlassSwitch]; it owns its own drag and tap, so a touch on the switch is
 * not seen a second time by the row.
 */
@Composable
internal fun SwitchRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    summary: String? = null,
    enabled: Boolean = true,
    titleColor: Color = TextPrimary,
    horizontalPadding: Dp = Space.md,
) {
    val haptic = LocalHapticFeedback.current
    val toggle: (Boolean) -> Unit = {
        haptic.performHapticFeedback(
            if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff,
        )
        onCheckedChange(it)
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .pressFeedback(enabled, Motion.PressedScaleRow, RoundedCornerShape(Radius.row))
            .toggleable(
                value = checked,
                enabled = enabled,
                role = Role.Switch,
                interactionSource = null,
                indication = null,
                onValueChange = toggle,
            )
            .graphicsLayer { alpha = if (enabled) 1f else TextAlpha.Disabled / TextAlpha.Primary }
            .heightIn(min = Space.rowMinHeight)
            .padding(horizontal = horizontalPadding, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text = title, color = titleColor, fontSize = TypeScale.body, lineHeight = 22.sp)
            if (!summary.isNullOrBlank()) {
                AnimatedContent(summary, transitionSpec = fadeThrough(), label = "switch-summary") {
                    Text(text = it, color = TextSecondary, fontSize = TypeScale.subhead, lineHeight = 20.sp)
                }
            }
        }
        GlassSwitch(checked = checked, enabled = enabled, onCheckedChange = toggle)
    }
}

/**
 * iOS 27 switch, 62x28dp: the vendored Kyant [LiquidToggle], whose thumb turns into a refracting
 * lens while it is dragged. The thumb samples a small capsule layer holding only the surface
 * colour, so it never reads the page it is drawn in. Disabled, with reduced motion, or without
 * live glass it draws the same flat capsule statically and leaves input to the row.
 * Its own semantics are cleared: the row is the single accessible switch.
 */
@Composable
internal fun GlassSwitch(checked: Boolean, enabled: Boolean, onCheckedChange: (Boolean) -> Unit) {
    val reduced = rememberReducedMotion()
    if (!enabled || reduced || !glassLive()) {
        FlatSwitch(checked = checked, reduced = reduced)
        return
    }
    val surface = Surfaces.card
    val local = rememberLayerBackdrop {
        drawRect(surface)
        drawContent()
    }
    val current by rememberUpdatedState(checked)
    val onChange by rememberUpdatedState(onCheckedChange)
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    // The toggle's drag inspector leaves pointer changes unconsumed; consume them here, after the
    // toggle has seen them, so the row's own toggleable does not act on the same tap again.
    // The toggle only listens on its thumb, so a tap that lands on the exposed track is turned
    // into a toggle here; thumb taps and drags stay with the toggle itself.
    Box(
        Modifier
            .size(SwitchWidth, SwitchHeight)
            .clearAndSetSemantics { }
            .pointerInput(rtl) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    down.consume()
                    val travel = if (current) SwitchWidth - SwitchThumbWidth - 4.dp else 0.dp
                    val thumbStart = (2.dp + travel).toPx()
                    val thumbEnd = thumbStart + SwitchThumbWidth.toPx()
                    val x = if (rtl) size.width - down.position.x else down.position.x
                    val onTrack = x < thumbStart || x > thumbEnd
                    val slop = viewConfiguration.touchSlop
                    var tap = true
                    do {
                        val event = awaitPointerEvent()
                        event.changes.forEach { change ->
                            if ((change.position - down.position).getDistance() > slop) tap = false
                            change.consume()
                        }
                    } while (event.changes.any { it.pressed })
                    if (tap && onTrack) onChange(!current)
                }
            },
    ) {
        Box(
            Modifier
                .matchParentSize()
                .clip(Capsule())
                .layerBackdrop(local)
                .background(surface),
        )
        LiquidToggle(
            selected = { current },
            onSelect = { if (it != current) onChange(it) },
            backdrop = local,
        )
    }
}

/** Static iOS switch: green or quiet track, white thumb with a soft shadow. */
@Composable
private fun FlatSwitch(checked: Boolean, reduced: Boolean) {
    val track by animateColorAsState(
        if (checked) SystemGreen else Surfaces.switchOff,
        if (reduced) snap() else Motion.effects(),
        label = "switch-track",
    )
    val knob by animateFloatAsState(
        if (checked) 1f else 0f,
        if (reduced) snap() else Motion.spatial(),
        label = "switch-knob",
    )
    val travel = with(LocalDensity.current) { (SwitchWidth - SwitchThumbWidth - 4.dp).toPx() }
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    Box(
        Modifier
            .size(SwitchWidth, SwitchHeight)
            .clip(Capsule())
            .background(track),
    ) {
        Box(
            Modifier
                .padding(2.dp)
                .size(SwitchThumbWidth, SwitchHeight - 4.dp)
                .graphicsLayer { translationX = knob * travel * if (rtl) -1f else 1f }
                .shadow(2.dp, Capsule())
                .background(Color.White, Capsule()),
        )
    }
}

private val SwitchWidth = 62.dp
private val SwitchHeight = 28.dp
private val SwitchThumbWidth = 38.dp

/** App icon on a squircle-ish clip; the letter tile cross-fades to the bitmap once decoded. */
@Composable
internal fun AppIcon(
    label: String,
    icon: Drawable?,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val bitmap = remember(icon) {
        icon?.toBitmap(width = 128, height = 128)?.asImageBitmap()
    }
    val shape = RoundedCornerShape(size * 0.24f)
    if (bitmap != null) {
        Image(
            bitmap = bitmap,
            contentDescription = null,
            modifier = modifier
                .size(size)
                .clip(shape),
        )
    } else {
        Box(
            modifier = modifier
                .size(size)
                .clip(shape)
                .background(Tone.Info.container()),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = label.trim().take(1).ifEmpty { "?" },
                color = Tone.Info.onContainer(),
                fontSize = TypeScale.title,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}
