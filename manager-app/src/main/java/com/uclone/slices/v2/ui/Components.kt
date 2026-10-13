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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.isSystemInDarkTheme
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
import androidx.compose.foundation.layout.WindowInsetsSides
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
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.union
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
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
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
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.graphics.drawable.toBitmap
import com.kyant.backdrop.backdrops.LayerBackdrop
import com.kyant.backdrop.backdrops.layerBackdrop
import com.kyant.backdrop.backdrops.rememberLayerBackdrop
import com.kyant.shapes.Capsule
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.liquid.LiquidButton
import com.uclone.slices.v2.ui.liquid.LiquidToggle
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

// ---------- Screen scaffold ----------

/**
 * Screen scaffold shared by every page, iOS 27 style: the list scrolls under a floating title bar
 * made of Liquid Glass buttons. A 34sp large title scrolls with the content; once it has scrolled
 * away the 17sp title and a soft edge fade in behind the buttons. The collapse fraction is read in
 * the draw phase only, so scrolling never recomposes the bar.
 *
 * The list is recorded into [backdrop] and the bar's glass refracts it. Pass a [backdrop] the
 * screen also provides to its own sheets (see [LocalGlassBackdrop]); page content always draws
 * solid surfaces.
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
    backdrop: LayerBackdrop = rememberGlassBackdrop(),
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
    val top = contentPadding.calculateTopPadding()
    Box(
        modifier
            .fillMaxSize()
            .background(Surfaces.page)
            .padding(
                start = contentPadding.calculateStartPadding(layoutDirection),
                end = contentPadding.calculateEndPadding(layoutDirection),
            ),
    ) {
        CompositionLocalProvider(LocalGlassBackdrop provides null) {
            LazyColumn(
                state = listState,
                modifier = Modifier
                    .fillMaxSize()
                    .glassSource(backdrop),
                contentPadding = PaddingValues(
                    top = top + Space.topBar,
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
        TopBar(
            title = title,
            collapse = collapse,
            statusBarHeight = top,
            backdrop = backdrop,
            onBack = onBack,
            backEnabled = backEnabled,
            backDescription = backDescription,
            actions = actions,
        )
    }
}

@Composable
private fun TopBar(
    title: String,
    collapse: () -> Float,
    statusBarHeight: Dp,
    backdrop: LayerBackdrop,
    onBack: (() -> Unit)?,
    backEnabled: Boolean,
    backDescription: String,
    actions: (@Composable RowScope.() -> Unit)?,
) {
    val page = Surfaces.page
    Box(Modifier.fillMaxWidth()) {
        // Scroll-edge effect: the page colour fades in behind the bar as the large title leaves.
        Box(
            Modifier
                .matchParentSize()
                .graphicsLayer { alpha = collapse() }
                .background(
                    Brush.verticalGradient(
                        0f to page,
                        .55f to page.copy(alpha = .86f),
                        1f to page.copy(alpha = 0f),
                    ),
                ),
        )
        CompositionLocalProvider(LocalGlassBackdrop provides backdrop) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = statusBarHeight)
                    .height(Space.topBar)
                    .padding(horizontal = Space.md - Space.xxs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (onBack != null) {
                    GlassBackButton(
                        onClick = onBack,
                        enabled = backEnabled,
                        contentDescription = backDescription,
                    )
                }
                Text(
                    text = title,
                    color = TextPrimary,
                    fontSize = TypeScale.collapsedTitle,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = if (onBack != null) TextAlign.Center else TextAlign.Start,
                    modifier = Modifier
                        .weight(1f)
                        .padding(horizontal = Space.xs)
                        .graphicsLayer {
                            val c = collapse()
                            alpha = c
                            translationY = (1f - c) * 6.dp.toPx()
                        },
                )
                if (actions != null) {
                    GlassButtonGroup(content = actions)
                } else if (onBack != null) {
                    // Keeps the centred title centred against the back button.
                    Spacer(Modifier.size(Space.glassButton))
                }
            }
        }
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
            lineHeight = 41.sp,
            letterSpacing = (-0.4).sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        accessory?.invoke(this)
    }
}

/**
 * Round 44dp Liquid Glass button carrying one glyph, as in iOS 27 navigation bars. It refracts
 * [LocalGlassBackdrop]; without a live backdrop it draws the same circle as solid glass.
 */
@Composable
internal fun GlassIconButton(
    painter: Painter,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = TextPrimary,
    iconRotation: Float = 0f,
) {
    val backdrop = LocalGlassBackdrop.current
    val fade by animateFloatAsState(if (enabled) 1f else TextAlpha.Disabled, Motion.effects(), label = "glass-icon-alpha")
    val glyph: @Composable () -> Unit = {
        Icon(
            painter = painter,
            contentDescription = contentDescription,
            tint = tint,
            modifier = Modifier
                .size(22.dp)
                .graphicsLayer { rotationZ = iconRotation },
        )
    }
    if (backdrop != null && glassLive()) {
        LiquidButton(
            onClick = onClick,
            backdrop = backdrop,
            modifier = modifier
                .graphicsLayer { alpha = fade }
                .size(Space.glassButton),
            enabled = enabled,
            surfaceColor = Surfaces.glassTint,
            buttonHeight = Space.glassButton,
            horizontalPadding = 0.dp,
        ) { glyph() }
    } else {
        Box(
            modifier = modifier
                .graphicsLayer { alpha = fade }
                .size(Space.glassButton)
                .pressFeedback(enabled, Motion.PressedScale, CircleShape)
                .solidGlass(CircleShape)
                .clickable(
                    enabled = enabled,
                    role = Role.Button,
                    interactionSource = null,
                    indication = null,
                    onClick = onClick,
                ),
            contentAlignment = Alignment.Center,
        ) { glyph() }
    }
}

/** iOS 27 back button: a round glass button showing only the chevron (‹, mirrored in RTL). */
@Composable
private fun GlassBackButton(onClick: () -> Unit, enabled: Boolean, contentDescription: String) {
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    GlassIconButton(
        painter = painterResource(R.drawable.ic_chevron_right),
        contentDescription = contentDescription,
        onClick = onClick,
        enabled = enabled,
        iconRotation = if (rtl) 0f else 180f,
    )
}

/**
 * Title-bar actions grouped on one glass capsule, as iOS 27 groups bar buttons. The items inside
 * are plain glyph buttons ([TopBarIconButton]) or flat [AppButton]s; only the capsule refracts.
 */
@Composable
private fun GlassButtonGroup(content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = Modifier
            .height(Space.glassButton)
            .glassSurface(RoundedCornerShape(Radius.chip), fallbackElevation = 0.dp)
            .padding(horizontal = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CompositionLocalProvider(LocalGlassBackdrop provides null) {
            content()
        }
    }
}

/** 44dp glyph action inside the title bar's glass group; round press feedback. */
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
            .size(Space.glassButton)
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
