package com.uclone.slices.v2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.LocalContentColor as MaterialLocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import top.yukonga.miuix.kmp.basic.Button as MiuixButton
import top.yukonga.miuix.kmp.basic.ButtonDefaults as MiuixButtonDefaults
import top.yukonga.miuix.kmp.basic.Card as MiuixCard
import top.yukonga.miuix.kmp.basic.CardDefaults as MiuixCardDefaults
import top.yukonga.miuix.kmp.basic.CircularProgressIndicator as MiuixCircularProgressIndicator
import top.yukonga.miuix.kmp.basic.Icon as MiuixIcon
import top.yukonga.miuix.kmp.basic.IconButton as MiuixIconButton
import top.yukonga.miuix.kmp.basic.ListPopup as MiuixListPopup
import top.yukonga.miuix.kmp.basic.ListPopupColumn as MiuixListPopupColumn
import top.yukonga.miuix.kmp.basic.PopupPositionProvider as MiuixPopupPositionProvider
import top.yukonga.miuix.kmp.basic.ProgressIndicatorDefaults as MiuixProgressIndicatorDefaults
import top.yukonga.miuix.kmp.basic.Switch as MiuixSwitch
import top.yukonga.miuix.kmp.basic.Text as MiuixText
import top.yukonga.miuix.kmp.basic.TextButton as MiuixTextButton
import top.yukonga.miuix.kmp.basic.TextField as MiuixTextField
import top.yukonga.miuix.kmp.extra.DropdownDefaults as MiuixDropdownDefaults
import top.yukonga.miuix.kmp.extra.DropdownImpl as MiuixDropdownItem
import top.yukonga.miuix.kmp.theme.MiuixTheme
import top.yukonga.miuix.kmp.utils.PressFeedbackType

@Composable
internal fun SlicesPanel(
    modifier: Modifier = Modifier,
    cornerRadius: Dp = 20.dp,
    insideMargin: PaddingValues = PaddingValues(0.dp),
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    color: Color = MiuixTheme.colorScheme.surfaceContainer,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = MiuixCardDefaults.defaultColors(
        color = color,
        contentColor = MiuixTheme.colorScheme.onSurfaceContainer,
    )
    // Material Text/Icon inside the card follow the card's content colour too.
    val panelContent: @Composable ColumnScope.() -> Unit = {
        CompositionLocalProvider(MaterialLocalContentColor provides colors.contentColor) {
            content()
        }
    }
    if (enabled && onClick != null) {
        MiuixCard(
            modifier = modifier,
            cornerRadius = cornerRadius,
            insideMargin = insideMargin,
            colors = colors,
            pressFeedbackType = PressFeedbackType.Sink,
            showIndication = true,
            onClick = onClick,
            content = panelContent,
        )
    } else {
        MiuixCard(
            modifier = modifier,
            cornerRadius = cornerRadius,
            insideMargin = insideMargin,
            colors = colors,
            content = panelContent,
        )
    }
}

@Composable
internal fun SlicesActionButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    primary: Boolean = true,
    danger: Boolean = false,
    icon: Painter? = null,
    compact: Boolean = false,
    loading: Boolean = false,
) {
    val buttonColor = when {
        danger -> MaterialTheme.colorScheme.error
        primary -> MiuixTheme.colorScheme.primary
        else -> MiuixTheme.colorScheme.secondaryVariant
    }
    val contentColor = when {
        !enabled -> MiuixTheme.colorScheme.onSurfaceVariantSummary
        danger -> MaterialTheme.colorScheme.onError
        primary -> MiuixTheme.colorScheme.onPrimary
        else -> MiuixTheme.colorScheme.onSecondaryVariant
    }
    MiuixButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        minHeight = if (compact) 40.dp else 48.dp,
        cornerRadius = 16.dp,
        insideMargin = if (compact) {
            PaddingValues(horizontal = 12.dp, vertical = 8.dp)
        } else {
            PaddingValues(horizontal = 16.dp, vertical = 12.dp)
        },
        colors = MiuixButtonDefaults.buttonColors(
            color = buttonColor,
            disabledColor = if (primary || danger) {
                MiuixTheme.colorScheme.disabledPrimaryButton
            } else {
                MiuixTheme.colorScheme.disabledSecondaryVariant
            },
        ),
    ) {
        if (loading) {
            SlicesSpinner(size = 18.dp, color = contentColor)
            Spacer(Modifier.width(8.dp))
        } else {
            icon?.let {
                MiuixIcon(
                    painter = it,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(20.dp),
                )
                Spacer(Modifier.width(8.dp))
            }
        }
        MiuixText(
            text = text,
            color = contentColor,
            style = MiuixTheme.textStyles.button,
        )
    }
}

@Composable
internal fun SlicesTextAction(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    primary: Boolean = false,
    danger: Boolean = false,
) {
    val colors = when {
        danger -> MiuixButtonDefaults.textButtonColors(
            color = MaterialTheme.colorScheme.error,
            disabledColor = MiuixTheme.colorScheme.disabledPrimaryButton,
            textColor = MaterialTheme.colorScheme.onError,
            disabledTextColor = MiuixTheme.colorScheme.onSurfaceVariantSummary,
        )
        primary -> MiuixButtonDefaults.textButtonColorsPrimary()
        else -> MiuixButtonDefaults.textButtonColors()
    }
    MiuixTextButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = colors,
    )
}

@Composable
internal fun SlicesInlineAction(
    text: String,
    onClick: () -> Unit,
    color: Color,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    MiuixTextButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        minWidth = 0.dp,
        minHeight = 32.dp,
        cornerRadius = 12.dp,
        insideMargin = PaddingValues(horizontal = 10.dp, vertical = 5.dp),
        colors = MiuixButtonDefaults.textButtonColors(
            color = Color.Transparent,
            disabledColor = Color.Transparent,
            textColor = color,
            disabledTextColor = color.copy(alpha = 0.38f),
        ),
    )
}

@Composable
internal fun SlicesHeaderButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    MiuixButton(
        onClick = onClick,
        modifier = modifier,
        minWidth = 0.dp,
        minHeight = 32.dp,
        cornerRadius = 12.dp,
        insideMargin = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
        colors = MiuixButtonDefaults.buttonColors(
            color = MiuixTheme.colorScheme.secondaryVariant,
        ),
    ) {
        MiuixText(
            text = text,
            color = MiuixTheme.colorScheme.onSecondaryVariant,
            style = MaterialTheme.typography.labelLarge,
        )
    }
}

@Composable
internal fun SlicesSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    iconRes: Int,
    modifier: Modifier = Modifier,
) {
    MiuixTextField(
        value = value,
        onValueChange = onValueChange,
        label = label,
        useLabelAsPlaceholder = true,
        singleLine = true,
        insideMargin = DpSize(15.dp, 14.dp),
        leadingIcon = {
            MiuixIcon(
                painter = painterResource(iconRes),
                contentDescription = null,
                tint = MiuixTheme.colorScheme.onSurfaceVariantActions,
            )
        },
        modifier = modifier,
    )
}

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
        useLabelAsPlaceholder = true,
        singleLine = true,
        insideMargin = DpSize(15.dp, 14.dp),
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
        useLabelAsPlaceholder = true,
        singleLine = true,
        insideMargin = DpSize(15.dp, 14.dp),
        modifier = modifier,
    )
}

@Composable
internal fun SlicesConfirmationToggle(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    text: String,
    modifier: Modifier = Modifier,
    textColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .toggleable(
                value = checked,
                role = Role.Switch,
                onValueChange = onCheckedChange,
            )
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = text,
            modifier = Modifier.weight(1f),
            color = textColor,
            style = MaterialTheme.typography.bodyMedium,
        )
        MiuixSwitch(
            checked = checked,
            onCheckedChange = null,
        )
    }
}

/** MIUIX indeterminate spinner; [color] defaults to the theme primary. */
@Composable
internal fun SlicesSpinner(
    size: Dp,
    modifier: Modifier = Modifier,
    color: Color = MiuixTheme.colorScheme.primary,
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
                                contentColor = MiuixTheme.colorScheme.error,
                            )
                        } else {
                            MiuixDropdownDefaults.dropdownColors()
                        },
                        onSelectedIndexChange = {
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
    Box(modifier) {
        MiuixIconButton(
            onClick = { show.value = true },
            enabled = enabled && items.isNotEmpty(),
            holdDownState = show.value,
        ) {
            MiuixIcon(
                painter = painterResource(R.drawable.ic_more_vert),
                contentDescription = contentDescription,
                tint = MiuixTheme.colorScheme.onSurfaceVariantActions,
            )
        }
        SlicesPopupMenu(show = show, items = items)
    }
}

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
    val contentColor = when {
        loading && selected -> Color.White
        loading -> MiuixTheme.colorScheme.onSecondaryVariant
        !enabled -> MiuixTheme.colorScheme.onSurfaceVariantSummary
        selected -> Color.White
        else -> MiuixTheme.colorScheme.onSecondaryVariant
    }
    MiuixButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier,
        cornerRadius = 18.dp,
        minHeight = 40.dp,
        minWidth = 0.dp,
        insideMargin = PaddingValues(horizontal = 14.dp, vertical = 9.dp),
        colors = MiuixButtonDefaults.buttonColors(
            color = if (selected) SlicesWatermelon else MiuixTheme.colorScheme.secondaryVariant,
            disabledColor = MiuixTheme.colorScheme.disabledSecondaryVariant,
        ),
    ) {
        if (loading) {
            SlicesSpinner(size = 16.dp, color = contentColor)
            Spacer(Modifier.width(7.dp))
        } else {
            icon?.let {
                MiuixIcon(
                    painter = it,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(7.dp))
            }
        }
        MiuixText(
            text = text,
            color = contentColor,
            style = MiuixTheme.textStyles.button,
            maxLines = 1,
        )
    }
}
