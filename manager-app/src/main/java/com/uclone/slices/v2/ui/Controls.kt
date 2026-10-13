package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.progressSemantics
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.uclone.slices.v2.R

/*
 * iOS 27 controls drawn with Compose foundation only: grouped panels, fields, progress, menus and
 * account chips. Glass chrome (title bar, sheets, alerts) lives in Components.kt / Glass.kt.
 */

/**
 * Grouped surface (radius 26 by default, no shadow). With [onClick] the whole panel gets press
 * feedback (0.965 + state layer), so every pressable surface in the app reacts the same way.
 */
@Composable
internal fun SlicesPanel(
    modifier: Modifier = Modifier,
    cornerRadius: Dp = Radius.card,
    insideMargin: PaddingValues = PaddingValues(0.dp),
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    color: Color = Surfaces.card,
    content: @Composable ColumnScope.() -> Unit,
) {
    val shape = RoundedCornerShape(cornerRadius)
    val interactive = onClick != null
    Column(
        modifier = if (interactive) {
            modifier
                .pressFeedback(enabled, Motion.PressedScale, shape)
                .clip(shape)
                .background(color)
                .clickable(
                    enabled = enabled,
                    role = Role.Button,
                    interactionSource = null,
                    indication = null,
                ) { onClick?.invoke() }
                .padding(insideMargin)
        } else {
            modifier
                .clip(shape)
                .background(color)
                .padding(insideMargin)
        },
    ) {
        // Material Text/Icon inside the panel follow the label colour.
        CompositionLocalProvider(LocalContentColor provides TextPrimary) {
            content()
        }
    }
}

@Composable
private fun fieldTextStyle(): TextStyle = TextStyle(color = TextPrimary, fontSize = TypeScale.body)

/**
 * Shared field chrome: quiet fill, [shape], the label as placeholder while empty and an accent
 * outline that fades in on focus.
 */
@Composable
private fun FieldDecoration(
    empty: Boolean,
    label: String,
    focused: Boolean,
    shape: Shape,
    leading: (@Composable () -> Unit)?,
    trailing: (@Composable () -> Unit)?,
    innerTextField: @Composable () -> Unit,
) {
    val outline by animateColorAsState(
        if (focused) Accent else Accent.copy(alpha = 0f),
        Motion.effects(),
        label = "field-outline",
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 50.dp)
            .clip(shape)
            .background(Surfaces.control)
            .border(1.5.dp, outline, shape)
            .padding(
                start = if (leading != null) Space.sm else Space.md,
                end = if (trailing != null) Space.xxs else Space.md,
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        leading?.invoke()
        Box(
            modifier = Modifier
                .weight(1f)
                .padding(vertical = 13.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            if (empty) {
                Text(
                    text = label,
                    color = TextTertiary,
                    fontSize = TypeScale.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            innerTextField()
        }
        trailing?.invoke()
    }
}

@Composable
private fun StringField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier,
    shape: Shape,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        textStyle = fieldTextStyle(),
        keyboardOptions = keyboardOptions,
        singleLine = true,
        visualTransformation = visualTransformation,
        interactionSource = interaction,
        cursorBrush = SolidColor(Accent),
        decorationBox = { inner ->
            FieldDecoration(
                empty = value.isEmpty(),
                label = label,
                focused = focused,
                shape = shape,
                leading = leading,
                trailing = trailing,
                innerTextField = inner,
            )
        },
    )
}

/** Text input: rounded field on the quiet fill; accent outline animates on focus. */
@Composable
internal fun SlicesInputField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    StringField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        modifier = modifier,
        shape = RoundedCornerShape(Radius.field),
    )
}

@Composable
internal fun SlicesInputField(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(Radius.field)
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier,
        textStyle = fieldTextStyle(),
        singleLine = true,
        interactionSource = interaction,
        cursorBrush = SolidColor(Accent),
        decorationBox = { inner ->
            FieldDecoration(
                empty = value.text.isEmpty(),
                label = label,
                focused = focused,
                shape = shape,
                leading = null,
                trailing = null,
                innerTextField = inner,
            )
        },
    )
}

/** Password input; same look as [SlicesInputField]. */
@Composable
internal fun SlicesPasswordField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    StringField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        modifier = modifier,
        shape = RoundedCornerShape(Radius.field),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        visualTransformation = PasswordVisualTransformation(),
    )
}

/** Search field: capsule on the quiet fill, leading search glyph, clear action when filled. */
@Composable
internal fun SlicesSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    clearDescription: String? = null,
) {
    StringField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        modifier = modifier,
        shape = RoundedCornerShape(Radius.control),
        leading = {
            Icon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = null,
                tint = TextSecondary,
                modifier = Modifier.size(20.dp),
            )
        },
        trailing = if (value.isNotEmpty() && clearDescription != null) {
            {
                Box(
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .clickable(role = Role.Button, onClickLabel = clearDescription) { onValueChange("") },
                    contentAlignment = Alignment.Center,
                ) {
                    Box(
                        modifier = Modifier
                            .size(18.dp)
                            .clip(CircleShape)
                            .background(TextTertiary),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            text = "✕",
                            color = Surfaces.card,
                            fontSize = TypeScale.caption,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        } else {
            null
        },
    )
}

/** Indeterminate activity ring; [color] defaults to the accent. */
@Composable
internal fun SlicesSpinner(
    size: Dp,
    modifier: Modifier = Modifier,
    color: Color = Accent,
    strokeWidth: Dp = 2.dp,
) {
    val transition = rememberInfiniteTransition(label = "spinner")
    val rotation = transition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(SpinnerTurnMs, easing = LinearEasing)),
        label = "spinner-rotation",
    )
    Canvas(
        modifier
            .progressSemantics()
            .size(size)
            .graphicsLayer { rotationZ = rotation.value },
    ) {
        val stroke = strokeWidth.toPx()
        drawArc(
            color = color,
            startAngle = 0f,
            sweepAngle = 270f,
            useCenter = false,
            topLeft = Offset(stroke / 2f, stroke / 2f),
            size = Size(this.size.width - stroke, this.size.height - stroke),
            style = Stroke(width = stroke, cap = StrokeCap.Round),
        )
    }
}

private const val SpinnerTurnMs = 900

/** Determinate progress: a 6dp capsule track with the accent fill. */
@Composable
internal fun SlicesLinearProgress(
    progress: Float,
    modifier: Modifier = Modifier,
) {
    val fraction = progress.coerceIn(0f, 1f)
    val shape = RoundedCornerShape(Radius.chip)
    Box(
        modifier
            .progressSemantics(fraction)
            .height(6.dp)
            .clip(shape)
            .background(Surfaces.control),
    ) {
        Box(
            Modifier
                .fillMaxHeight()
                .fillMaxWidth(fraction)
                .clip(shape)
                .background(Accent),
        )
    }
}

internal data class SlicesMenuItem(
    val text: String,
    val onClick: () -> Unit,
    /** Shown with a check mark, for on/off options inside a menu. */
    val checked: Boolean = false,
    val danger: Boolean = false,
)

/**
 * iOS context menu anchored to its parent layout, so it must be placed in the same Box as the
 * control that opens it. Opens below the anchor's end edge and flips above it when there is no
 * room below. Scales in from its top end corner; reduced motion shows it at once.
 */
@Composable
internal fun SlicesPopupMenu(
    show: MutableState<Boolean>,
    items: List<SlicesMenuItem>,
) {
    val haptic = LocalHapticFeedback.current
    val reduced = rememberReducedMotion()
    val visibleState = remember { MutableTransitionState(false) }
    val open = show.value
    LaunchedEffect(open) { visibleState.targetState = open }
    if (!visibleState.currentState && !visibleState.targetState) return
    val margin = with(LocalDensity.current) { Space.xs.roundToPx() }
    val positionProvider = remember(margin) { MenuPositionProvider(margin) }
    Popup(
        popupPositionProvider = positionProvider,
        onDismissRequest = { show.value = false },
        properties = PopupProperties(focusable = true),
    ) {
        AnimatedVisibility(
            visibleState = visibleState,
            enter = if (reduced) {
                EnterTransition.None
            } else {
                fadeIn(Motion.effects()) +
                    scaleIn(Motion.spatial(), initialScale = .8f, transformOrigin = TransformOrigin(1f, 0f))
            },
            exit = if (reduced) {
                ExitTransition.None
            } else {
                fadeOut(Motion.exit()) +
                    scaleOut(Motion.exit(), targetScale = .9f, transformOrigin = TransformOrigin(1f, 0f))
            },
        ) {
            Column(
                modifier = Modifier
                    .padding(Space.xs)
                    .widthIn(min = 200.dp, max = 300.dp)
                    .width(IntrinsicSize.Max)
                    .solidGlass(RoundedCornerShape(Radius.menu), Surfaces.panelTint, elevation = 12.dp)
                    .padding(vertical = 6.dp),
            ) {
                items.forEachIndexed { index, item ->
                    key(index, item.text) {
                        if (index > 0) InsetHairline(start = Space.md)
                        MenuRow(
                            item = item,
                            onClick = {
                                haptic.performHapticFeedback(HapticFeedbackType.SegmentTick)
                                show.value = false
                                item.onClick()
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun MenuRow(item: SlicesMenuItem, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .pressFeedback(true, 1f, RectangleShape)
            .clickable(
                role = Role.Button,
                interactionSource = null,
                indication = null,
                onClick = onClick,
            )
            .semantics { selected = item.checked }
            .heightIn(min = 46.dp)
            .padding(horizontal = Space.md, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(
            text = item.text,
            color = if (item.danger) SystemRed else TextPrimary,
            fontSize = TypeScale.body,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (item.checked) {
            Text(
                text = "✓",
                color = Accent,
                fontSize = TypeScale.body,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

/** Places the menu under the anchor's end edge, flipping above it when it would not fit. */
private class MenuPositionProvider(private val margin: Int) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val preferredX = if (layoutDirection == LayoutDirection.Ltr) {
            anchorBounds.right - popupContentSize.width
        } else {
            anchorBounds.left
        }
        val maxX = (windowSize.width - popupContentSize.width - margin).coerceAtLeast(margin)
        val x = preferredX.coerceIn(margin, maxX)
        val fitsBelow = anchorBounds.bottom + popupContentSize.height + margin <= windowSize.height
        val y = if (fitsBelow) {
            anchorBounds.bottom
        } else {
            (anchorBounds.top - popupContentSize.height).coerceAtLeast(margin)
        }
        return IntOffset(x, y)
    }
}

/** The iOS "more" button used on cards (an ellipsis in a quiet circle), opening a [SlicesPopupMenu]. */
@Composable
internal fun SlicesOverflowMenu(
    items: List<SlicesMenuItem>,
    contentDescription: String,
    enabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val show = remember { mutableStateOf(false) }
    val active = enabled && items.isNotEmpty()
    val fill by animateColorAsState(
        if (show.value) Surfaces.stateLayer.compositeOver(Surfaces.control) else Surfaces.control,
        Motion.effects(),
        label = "overflow-fill",
    )
    Box(modifier) {
        Box(
            modifier = Modifier
                .size(Space.glassButton)
                .pressFeedback(active, Motion.PressedScale, CircleShape)
                .clip(CircleShape)
                .clickable(
                    enabled = active,
                    role = Role.Button,
                    interactionSource = null,
                    indication = null,
                ) { show.value = true },
            contentAlignment = Alignment.Center,
        ) {
            Box(
                modifier = Modifier
                    .size(30.dp)
                    .clip(CircleShape)
                    .background(fill),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_more_vert),
                    contentDescription = contentDescription,
                    tint = if (active) Accent else TextDisabled,
                    modifier = Modifier
                        .size(20.dp)
                        .graphicsLayer { rotationZ = 90f },
                )
            }
        }
        SlicesPopupMenu(show = show, items = items)
    }
}

/**
 * Account chip for quick switching: capsule, accent fill when it is the active account, quiet
 * fill otherwise. Colours follow the effects spring; press feedback 0.965.
 */
@Composable
internal fun SlicesSlotButton(
    text: String,
    icon: Painter? = null,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    loading: Boolean = false,
) {
    val container by animateColorAsState(
        if (selected) Accent else Surfaces.control,
        Motion.effects(),
        label = "slot-bg",
    )
    val content by animateColorAsState(
        if (selected) OnAccent else TextPrimary,
        Motion.effects(),
        label = "slot-fg",
    )
    val shape = RoundedCornerShape(Radius.chip)
    Row(
        modifier = modifier
            .pressFeedback(enabled && !loading, Motion.PressedScale, shape)
            .graphicsLayer { alpha = if (enabled || loading) 1f else TextAlpha.Disabled }
            .heightIn(min = 40.dp)
            .clip(shape)
            .background(container)
            .clickable(
                enabled = enabled && !loading,
                role = Role.Button,
                interactionSource = null,
                indication = null,
                onClick = onClick,
            )
            .padding(horizontal = 14.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp, Alignment.CenterHorizontally),
    ) {
        if (loading) {
            SlicesSpinner(size = 16.dp, color = content)
        } else {
            icon?.let {
                Icon(
                    painter = it,
                    contentDescription = null,
                    tint = content,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        Text(
            text = text,
            color = content,
            fontSize = TypeScale.subhead,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
