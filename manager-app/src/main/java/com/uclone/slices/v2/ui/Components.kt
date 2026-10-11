package com.uclone.slices.v2.ui

import android.graphics.drawable.Drawable
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.graphics.drawable.toBitmap
import com.uclone.slices.v2.R
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import top.yukonga.miuix.kmp.basic.Icon
import top.yukonga.miuix.kmp.basic.Switch
import top.yukonga.miuix.kmp.basic.Text

// ---------- Screen scaffold ----------

/**
 * Screen scaffold shared by every page: edge-to-edge page colour, a 30sp large title that scrolls
 * with the content and collapses into a 17sp title bar whose hairline fades in. The collapse
 * fraction is read in the draw phase only, so scrolling never recomposes the bar.
 */
@Composable
internal fun ManagerScreen(
    title: String,
    contentPadding: PaddingValues,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
    backEnabled: Boolean = true,
    backDescription: String = stringResource(R.string.back),
    titleAccessory: (@Composable RowScope.() -> Unit)? = null,
    actions: (@Composable RowScope.() -> Unit)? = null,
    listState: LazyListState = rememberLazyListState(),
    itemSpacing: Dp = Space.sm,
    content: LazyListScope.() -> Unit,
) {
    val threshold = with(LocalDensity.current) { 44.dp.toPx() }
    val collapse: () -> Float = {
        if (listState.firstVisibleItemIndex > 0) {
            1f
        } else {
            (listState.firstVisibleItemScrollOffset / threshold).coerceIn(0f, 1f)
        }
    }
    val layoutDirection = LocalLayoutDirection.current
    Column(
        modifier
            .fillMaxSize()
            .background(Surfaces.page)
            .padding(
                start = contentPadding.calculateStartPadding(layoutDirection),
                end = contentPadding.calculateEndPadding(layoutDirection),
            ),
    ) {
        TopBar(
            title = title,
            collapse = collapse,
            modifier = Modifier.padding(top = contentPadding.calculateTopPadding()),
            onBack = onBack,
            backEnabled = backEnabled,
            backDescription = backDescription,
            actions = actions,
        )
        LazyColumn(
            state = listState,
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
            contentPadding = PaddingValues(
                bottom = contentPadding.calculateBottomPadding() + Space.xl,
            ),
            verticalArrangement = Arrangement.spacedBy(itemSpacing),
        ) {
            item(key = "screen_large_title", contentType = "large_title") {
                LargeTitle(title, accessory = titleAccessory)
            }
            content()
        }
    }
}

@Composable
private fun TopBar(
    title: String,
    collapse: () -> Float,
    modifier: Modifier,
    onBack: (() -> Unit)?,
    backEnabled: Boolean,
    backDescription: String,
    actions: (@Composable RowScope.() -> Unit)?,
) {
    val hairline = Surfaces.hairline
    Column(modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(Space.touchTarget + Space.xxs)
                .padding(horizontal = Space.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                TopBarIconButton(
                    painter = painterResource(R.drawable.ic_arrow_back),
                    contentDescription = backDescription,
                    onClick = onBack,
                    enabled = backEnabled,
                    tint = TextPrimary,
                )
            }
            Text(
                text = title,
                color = TextPrimary,
                fontSize = TypeScale.collapsedTitle,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .weight(1f)
                    .padding(start = if (onBack == null) Space.md else Space.xxs, end = Space.xs)
                    .graphicsLayer {
                        val c = collapse()
                        alpha = c
                        translationY = (1f - c) * 6.dp.toPx()
                    },
            )
            actions?.invoke(this)
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(0.5.dp)
                .graphicsLayer { alpha = collapse() }
                .background(hairline),
        )
    }
}

@Composable
private fun LargeTitle(text: String, accessory: (@Composable RowScope.() -> Unit)?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = Space.gutter + Space.xxs, end = Space.gutter, bottom = Space.xxs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(
            text = text,
            color = TextPrimary,
            fontSize = TypeScale.largeTitle,
            fontWeight = FontWeight.Bold,
            lineHeight = 36.sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        accessory?.invoke(this)
    }
}

/** 48dp icon action used in the title bar; round press feedback. */
@Composable
internal fun TopBarIconButton(
    painter: Painter,
    contentDescription: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
    tint: Color = Accent,
) {
    Box(
        modifier = Modifier
            .size(Space.touchTarget)
            .pressFeedback(enabled, Motion.PressedScale, CircleShape)
            .clip(CircleShape)
            .clickable(
                enabled = enabled,
                role = Role.Button,
                onClickLabel = contentDescription,
                interactionSource = null,
                indication = null,
                onClick = onClick,
            )
            .graphicsLayer { alpha = if (enabled) 1f else TextAlpha.Disabled },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painter,
            contentDescription = contentDescription,
            tint = tint,
            modifier = Modifier.size(22.dp),
        )
    }
}

// ---------- Sections and rows ----------

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
 * Grouped card: no dividers; rows are separated by their 56dp height and insets. Placed on the
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

/** Leading icon on a 36dp tonal plate (radius 10). */
@Composable
internal fun IconPlate(painter: Painter, tone: Tone = Tone.Info, size: Dp = 36.dp) {
    val container by animateColorAsState(tone.container(), Motion.effects(), label = "plate-bg")
    val content by animateColorAsState(tone.onContainer(), Motion.effects(), label = "plate-fg")
    Box(
        modifier = Modifier
            .size(size)
            .clip(RoundedCornerShape(Radius.small))
            .background(container),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painter = painter, contentDescription = null, tint = content, modifier = Modifier.size(size * 0.56f))
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
 * Switch row: the whole row toggles. The Miuix switch is display-only here (null callback), so
 * the row emits the toggleOn / toggleOff haptic exactly once.
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
            ) {
                haptic.performHapticFeedback(
                    if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff,
                )
                onCheckedChange(it)
            }
            .graphicsLayer { alpha = if (enabled) 1f else TextAlpha.Disabled / TextAlpha.Primary }
            .heightIn(min = Space.rowMinHeight)
            .padding(horizontal = horizontalPadding, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text = title, color = titleColor, fontSize = TypeScale.body, lineHeight = 21.sp)
            if (!summary.isNullOrBlank()) {
                AnimatedContent(summary, transitionSpec = fadeThrough(), label = "switch-summary") {
                    Text(text = it, color = TextSecondary, fontSize = TypeScale.subhead, lineHeight = 19.sp)
                }
            }
        }
        Switch(checked = checked, onCheckedChange = null, enabled = enabled)
    }
}

// ---------- Status ----------

/** Small status dot whose colour follows the tone with the effects spring. */
@Composable
internal fun StatusDot(tone: Tone, modifier: Modifier = Modifier, size: Dp = 10.dp) {
    val color by animateColorAsState(tone.onContainer(), Motion.effects(), label = "dot")
    Box(
        modifier
            .size(size)
            .clip(CircleShape)
            .background(color),
    )
}

/** StatusChip: tone container + tone text with a dot; colours animate, label fades through. */
@Composable
internal fun StatusChip(text: String, tone: Tone, modifier: Modifier = Modifier) {
    val container by animateColorAsState(tone.container(), Motion.effects(), label = "chip-bg")
    val content by animateColorAsState(tone.onContainer(), Motion.effects(), label = "chip-fg")
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(Radius.chip))
            .background(container)
            .padding(horizontal = 10.dp, vertical = 5.dp)
            .animateContentSize(Motion.size),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .size(6.dp)
                .clip(CircleShape)
                .background(content),
        )
        AnimatedContent(text, transitionSpec = fadeThrough(), label = "chip-text") {
            Text(text = it, color = content, fontSize = TypeScale.caption, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
    }
}

/**
 * Inline banner for warnings, results and recovery prompts: tone container, optional headline,
 * body text and an optional slot for actions.
 */
@Composable
internal fun Banner(
    text: String,
    tone: Tone,
    modifier: Modifier = Modifier,
    title: String? = null,
    gutter: Boolean = true,
    content: (@Composable ColumnScope.() -> Unit)? = null,
) {
    val container by animateColorAsState(tone.container(), Motion.effects(), label = "banner-bg")
    val onContainer by animateColorAsState(tone.onContainer(), Motion.effects(), label = "banner-fg")
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = if (gutter) Space.gutter else 0.dp)
            .clip(RoundedCornerShape(Radius.row))
            .background(container)
            .padding(horizontal = Space.md, vertical = Space.sm)
            .animateContentSize(Motion.size),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier
                .padding(top = 7.dp)
                .size(6.dp)
                .clip(CircleShape)
                .background(onContainer),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xxs)) {
            if (title != null) {
                Text(text = title, color = onContainer, fontSize = TypeScale.body, fontWeight = FontWeight.SemiBold)
            }
            AnimatedContent(text, transitionSpec = fadeThrough(), label = "banner-text") {
                Text(text = it, color = onContainer, fontSize = TypeScale.subhead, lineHeight = 20.sp)
            }
            content?.invoke(this)
        }
    }
}

/** Empty / error state: 64dp round tonal icon, headline title, one subhead line, optional action. */
@Composable
internal fun EmptyState(
    title: String,
    modifier: Modifier = Modifier,
    message: String? = null,
    icon: Painter = painterResource(R.drawable.ic_space_cube),
    tone: Tone = Tone.Neutral,
    action: String? = null,
    onAction: (() -> Unit)? = null,
    actionEnabled: Boolean = true,
) {
    StateLayout(modifier = modifier, title = title, message = message, action = action, onAction = onAction, actionEnabled = actionEnabled) {
        Box(
            Modifier
                .size(64.dp)
                .clip(CircleShape)
                .background(tone.container()),
            contentAlignment = Alignment.Center,
        ) {
            Icon(painter = icon, contentDescription = null, tint = tone.onContainer(), modifier = Modifier.size(28.dp))
        }
    }
}

/** Loading state: the same layout as [EmptyState] with a spinner in the tonal circle. */
@Composable
internal fun LoadingState(title: String, modifier: Modifier = Modifier, message: String? = null) {
    StateLayout(modifier = modifier, title = title, message = message, action = null, onAction = null, actionEnabled = false) {
        Box(
            Modifier
                .size(64.dp)
                .clip(CircleShape)
                .background(Tone.Info.container()),
            contentAlignment = Alignment.Center,
        ) {
            SlicesSpinner(size = 28.dp, color = Tone.Info.onContainer(), strokeWidth = 3.dp)
        }
    }
}

@Composable
private fun StateLayout(
    modifier: Modifier,
    title: String,
    message: String?,
    action: String?,
    onAction: (() -> Unit)?,
    actionEnabled: Boolean,
    icon: @Composable () -> Unit,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = Space.lg, vertical = Space.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        icon()
        Spacer(Modifier.height(Space.xxs))
        Text(
            text = title,
            color = TextPrimary,
            fontSize = TypeScale.headline,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
        )
        if (message != null) {
            Text(
                text = message,
                color = TextSecondary,
                fontSize = TypeScale.subhead,
                lineHeight = 20.sp,
                textAlign = TextAlign.Center,
            )
        }
        if (action != null && onAction != null) {
            Spacer(Modifier.height(Space.xxs))
            AppButton(
                text = action,
                onClick = onAction,
                modifier = Modifier.widthIn(min = 160.dp),
                kind = ButtonKind.Secondary,
                enabled = actionEnabled,
            )
        }
    }
}

// ---------- Buttons ----------

internal enum class ButtonKind { Primary, Secondary, Destructive, Text }

/**
 * Buttons per spec: primary filled 52dp / r16, secondary tonal (accent 12%), destructive (danger
 * tone), text. All get press feedback; primary and destructive confirm with a haptic, text
 * buttons that cancel a dangerous action can opt into the reject haptic.
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
        ButtonKind.Destructive -> Tone.Danger.onContainer()
    }
    val container by animateColorAsState(targetContainer, Motion.effects(), label = "button-bg")
    val content by animateColorAsState(targetContent, Motion.effects(), label = "button-fg")
    val fade by animateFloatAsState(if (enabled) 1f else TextAlpha.Disabled, Motion.effects(), label = "button-alpha")
    Box(
        modifier = modifier
            .pressFeedback(enabled && !loading, Motion.PressedScale, shape)
            .graphicsLayer { alpha = fade }
            .heightIn(min = if (compact) 40.dp else 52.dp)
            .clip(shape)
            .background(container)
            .clickable(
                enabled = enabled && !loading,
                role = Role.Button,
                interactionSource = null,
                indication = null,
            ) {
                when {
                    kind == ButtonKind.Primary || kind == ButtonKind.Destructive ->
                        haptic.performHapticFeedback(HapticFeedbackType.Confirm)
                    rejectHaptic -> haptic.performHapticFeedback(HapticFeedbackType.Reject)
                }
                onClick()
            }
            .padding(
                horizontal = if (compact) Space.sm else Space.lg,
                vertical = if (compact) Space.xs else Space.sm,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.xs),
        ) {
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
    }
}

/** Cancel + confirm pair used at the bottom of every confirmation sheet. */
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

/** Segmented control: the selected indicator slides on the spatial spring; a tick on change. */
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
    val indicator = Surfaces.raised
    val indicatorBorder = Surfaces.hairline
    BoxWithConstraints(
        modifier
            .fillMaxWidth()
            .height(Space.touchTarget)
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
        Box(
            Modifier
                .width(segment)
                .fillMaxHeight()
                .offset { IntOffset((position.value * segmentPx).roundToInt(), 0) }
                .clip(RoundedCornerShape(Radius.control - 3.dp))
                .background(indicator)
                .border(0.5.dp, indicatorBorder, RoundedCornerShape(Radius.control - 3.dp)),
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
                        fontWeight = FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(horizontal = Space.xs),
                    )
                }
            }
        }
    }
}

// ---------- Sheets and dialogs ----------

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
 * Bottom sheet used for every confirmation, choice and form: top radius 28, 36x4 drag handle,
 * drag-down or scrim tap to dismiss, predictive back. Enters with spring(0.9, 380); exits in
 * 220ms on the accelerating curve. It stays composed while the exit runs; callbacks only fire
 * while it is visible. [dismissible] = false keeps it open against back, drag and scrim.
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
    val sheetShape = RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet)
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
                .fillMaxWidth()
                .widthIn(max = 640.dp)
                .windowInsetsPadding(WindowInsets.ime)
                .onSizeChanged { height = it.height }
                .graphicsLayer { translationY = hidden.value * (if (height > 0) height.toFloat() else 2000f) }
                .clip(sheetShape)
                .background(Surfaces.raised)
                .border(0.5.dp, Surfaces.hairline, sheetShape)
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
                .windowInsetsPadding(WindowInsets.navigationBars)
                .padding(start = Space.md, end = Space.md, bottom = Space.md),
        ) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .padding(top = 10.dp, bottom = Space.sm),
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier
                        .size(width = 36.dp, height = 4.dp)
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
                        fontWeight = FontWeight.SemiBold,
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
 * Centred dialog, reserved for the short blocking "operation in progress" state: scale 0.94 -> 1
 * plus fade on spring(0.9, 500); exits on the 220ms accelerating curve. Not dismissible.
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
            .graphicsLayer { alpha = shown.value.coerceIn(0f, 1f) }
            .background(scrim)
            .clickable(interactionSource = null, indication = null) { },
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .padding(horizontal = Space.xl)
                .widthIn(max = 360.dp)
                .fillMaxWidth()
                .graphicsLayer {
                    val s = Motion.DialogInitialScale + (1f - Motion.DialogInitialScale) * shown.value
                    scaleX = s
                    scaleY = s
                }
                .clip(RoundedCornerShape(Radius.sheet))
                .background(Surfaces.raised)
                .border(0.5.dp, Surfaces.hairline, RoundedCornerShape(Radius.sheet))
                .padding(Space.lg),
            horizontalAlignment = Alignment.CenterHorizontally,
            content = content,
        )
    }
}

// ---------- Misc ----------

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
