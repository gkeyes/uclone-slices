package com.uclone.slices.v2.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.LocalContentColor as MaterialLocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import top.yukonga.miuix.kmp.basic.Card as MiuixCard
import top.yukonga.miuix.kmp.basic.CardDefaults as MiuixCardDefaults
import top.yukonga.miuix.kmp.basic.CircularProgressIndicator as MiuixCircularProgressIndicator
import top.yukonga.miuix.kmp.basic.Icon as MiuixIcon
import top.yukonga.miuix.kmp.basic.IconButton as MiuixIconButton
import top.yukonga.miuix.kmp.basic.ListPopup as MiuixListPopup
import top.yukonga.miuix.kmp.basic.ListPopupColumn as MiuixListPopupColumn
import top.yukonga.miuix.kmp.basic.PopupPositionProvider as MiuixPopupPositionProvider
import top.yukonga.miuix.kmp.basic.ProgressIndicatorDefaults as MiuixProgressIndicatorDefaults
import top.yukonga.miuix.kmp.basic.Text as MiuixText
import top.yukonga.miuix.kmp.basic.TextField as MiuixTextField
import top.yukonga.miuix.kmp.extra.DropdownDefaults as MiuixDropdownDefaults
import top.yukonga.miuix.kmp.extra.DropdownImpl as MiuixDropdownItem

/**
 * MIUIX card on the spec card surface (radius 24, no shadow). With [onClick] the whole card gets
 * press feedback (0.965 + state layer) instead of MIUIX's sink, so every pressable surface in the
 * app reacts the same way.
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
    val colors = MiuixCardDefaults.defaultColors(
        color = color,
        contentColor = TextPrimary,
    )
    // Material Text/Icon inside the card follow the card's content colour too.
    val panelContent: @Composable ColumnScope.() -> Unit = {
        CompositionLocalProvider(MaterialLocalContentColor provides colors.contentColor) {
            content()
        }
    }
    val shape = RoundedCornerShape(cornerRadius)
    val interactive = onClick != null
    MiuixCard(
        modifier = if (interactive) {
            modifier
                .pressFeedback(enabled, Motion.PressedScale, shape)
                .clip(shape)
                .clickable(
                    enabled = enabled,
                    role = Role.Button,
                    interactionSource = null,
                    indication = null,
                ) { onClick?.invoke() }
        } else {
            modifier
        },
        cornerRadius = cornerRadius,
        insideMargin = insideMargin,
        colors = colors,
        content = panelContent,
    )
}

@Composable
private fun fieldTextStyle(): TextStyle = TextStyle(color = TextPrimary, fontSize = TypeScale.body)

/** Text input: MIUIX field at radius 14 on the control tint; accent outline animates on focus. */
@Composable
internal fun SlicesInputField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    MiuixTextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        labelColor = TextTertiary,
        useLabelAsPlaceholder = true,
        singleLine = true,
        cornerRadius = Radius.control,
        backgroundColor = Surfaces.control,
        borderColor = Accent,
        textStyle = fieldTextStyle(),
        insideMargin = DpSize(Space.md, 14.dp),
        modifier = modifier,
    )
}

@Composable
internal fun SlicesInputField(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    MiuixTextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        labelColor = TextTertiary,
        useLabelAsPlaceholder = true,
        singleLine = true,
        cornerRadius = Radius.control,
        backgroundColor = Surfaces.control,
        borderColor = Accent,
        textStyle = fieldTextStyle(),
        insideMargin = DpSize(Space.md, 14.dp),
        modifier = modifier,
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
    MiuixTextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        labelColor = TextTertiary,
        useLabelAsPlaceholder = true,
        singleLine = true,
        cornerRadius = Radius.control,
        backgroundColor = Surfaces.control,
        borderColor = Accent,
        textStyle = fieldTextStyle(),
        insideMargin = DpSize(Space.md, 14.dp),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        visualTransformation = PasswordVisualTransformation(),
        modifier = modifier,
    )
}

/** Search field: radius 14, control tint, leading search icon, clear action when filled. */
@Composable
internal fun SlicesSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    clearDescription: String? = null,
) {
    MiuixTextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        labelColor = TextTertiary,
        useLabelAsPlaceholder = true,
        singleLine = true,
        cornerRadius = Radius.control,
        backgroundColor = Surfaces.control,
        borderColor = Accent,
        textStyle = fieldTextStyle(),
        insideMargin = DpSize(Space.sm, 13.dp),
        leadingIcon = {
            MiuixIcon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = null,
                tint = TextSecondary,
                modifier = Modifier
                    .padding(start = Space.sm)
                    .size(20.dp),
            )
        },
        trailingIcon = if (value.isNotEmpty() && clearDescription != null) {
            {
                Box(
                    modifier = Modifier
                        .padding(end = Space.xxs)
                        .size(40.dp)
                        .clip(CircleShape)
                        .clickable(role = Role.Button, onClickLabel = clearDescription) { onValueChange("") },
                    contentAlignment = Alignment.Center,
                ) {
                    MiuixText(text = "✕", color = TextSecondary, fontSize = TypeScale.subhead)
                }
            }
        } else {
            null
        },
        modifier = modifier,
    )
}

/** MIUIX indeterminate spinner; [color] defaults to the accent. */
@Composable
internal fun SlicesSpinner(
    size: Dp,
    modifier: Modifier = Modifier,
    color: Color = Accent,
    strokeWidth: Dp = 2.dp,
) {
    MiuixCircularProgressIndicator(
        modifier = modifier,
        size = size,
        strokeWidth = strokeWidth,
        colors = MiuixProgressIndicatorDefaults.progressIndicatorColors(
            foregroundColor = color,
            backgroundColor = Color.Transparent,
        ),
    )
}

internal data class SlicesMenuItem(
    val text: String,
    val onClick: () -> Unit,
    /** Shown with the MIUIX check mark, for on/off options inside a menu. */
    val checked: Boolean = false,
    val danger: Boolean = false,
)

/**
 * MIUIX popup list (the same one SuperDropdown opens) anchored to its parent layout, so it
 * must be placed in the same Box as the control that opens it. Flips above the anchor when
 * there is no room below.
 */
@Composable
internal fun SlicesPopupMenu(
    show: MutableState<Boolean>,
    items: List<SlicesMenuItem>,
) {
    val haptic = LocalHapticFeedback.current
    MiuixListPopup(
        show = show,
        alignment = MiuixPopupPositionProvider.Align.Right,
        onDismissRequest = { show.value = false },
    ) {
        MiuixListPopupColumn {
            items.forEachIndexed { index, item ->
                key(index, item.text) {
                    MiuixDropdownItem(
                        text = item.text,
                        optionSize = items.size,
                        isSelected = item.checked,
                        index = index,
                        dropdownColors = if (item.danger) {
                            MiuixDropdownDefaults.dropdownColors(
                                contentColor = Tone.Danger.onContainer(),
                            )
                        } else {
                            MiuixDropdownDefaults.dropdownColors()
                        },
                        onSelectedIndexChange = {
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

/** The "more" button used on cards, opening a [SlicesPopupMenu]. */
@Composable
internal fun SlicesOverflowMenu(
    items: List<SlicesMenuItem>,
    contentDescription: String,
    enabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val show = remember { mutableStateOf(false) }
    val active = enabled && items.isNotEmpty()
    Box(modifier) {
        MiuixIconButton(
            onClick = { show.value = true },
            enabled = active,
            holdDownState = show.value,
            modifier = Modifier.pressFeedback(active, Motion.PressedScale, CircleShape),
        ) {
            MiuixIcon(
                painter = painterResource(R.drawable.ic_more_vert),
                contentDescription = contentDescription,
                tint = if (active) TextSecondary else TextDisabled,
            )
        }
        SlicesPopupMenu(show = show, items = items)
    }
}

/**
 * Account chip for quick switching: full-round, accent fill when it is the active account,
 * control tint otherwise. Colours follow the effects spring; press feedback 0.965.
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
                MiuixIcon(
                    painter = it,
                    contentDescription = null,
                    tint = content,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        MiuixText(
            text = text,
            color = content,
            fontSize = TypeScale.subhead,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
