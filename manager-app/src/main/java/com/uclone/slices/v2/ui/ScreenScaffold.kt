package com.uclone.slices.v2.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kyant.backdrop.backdrops.LayerBackdrop
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.liquid.LiquidButton

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
