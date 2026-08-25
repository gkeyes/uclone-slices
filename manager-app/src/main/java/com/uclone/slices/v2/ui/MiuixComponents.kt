package com.uclone.slices.v2.ui

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import top.yukonga.miuix.kmp.basic.Button as MiuixButton
import top.yukonga.miuix.kmp.basic.ButtonDefaults as MiuixButtonDefaults
import top.yukonga.miuix.kmp.basic.Card as MiuixCard
import top.yukonga.miuix.kmp.basic.CardDefaults as MiuixCardDefaults
import top.yukonga.miuix.kmp.basic.Icon as MiuixIcon
import top.yukonga.miuix.kmp.basic.Text as MiuixText
import top.yukonga.miuix.kmp.basic.TextButton as MiuixTextButton
import top.yukonga.miuix.kmp.basic.TextField as MiuixTextField
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
    if (enabled && onClick != null) {
        MiuixCard(
            modifier = modifier,
            cornerRadius = cornerRadius,
            insideMargin = insideMargin,
            colors = colors,
            pressFeedbackType = PressFeedbackType.Sink,
            showIndication = true,
            onClick = onClick,
            content = content,
        )
    } else {
        MiuixCard(
            modifier = modifier,
            cornerRadius = cornerRadius,
            insideMargin = insideMargin,
            colors = colors,
            content = content,
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
        icon?.let {
            MiuixIcon(
                painter = it,
                contentDescription = null,
                tint = contentColor,
                modifier = Modifier.size(20.dp),
            )
            Spacer(Modifier.width(8.dp))
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
internal fun SlicesSlotButton(
    text: String,
    icon: Painter? = null,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val contentColor = when {
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
        icon?.let {
            MiuixIcon(
                painter = it,
                contentDescription = null,
                tint = contentColor,
                modifier = Modifier.size(18.dp),
            )
            Spacer(Modifier.width(7.dp))
        }
        MiuixText(
            text = text,
            color = contentColor,
            style = MiuixTheme.textStyles.button,
            maxLines = 1,
        )
    }
}
