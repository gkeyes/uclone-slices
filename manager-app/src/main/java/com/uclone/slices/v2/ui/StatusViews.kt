package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R

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

/**
 * StatusChip: tone container + tone text with a dot; colours animate, and a changing label fades
 * through. Pass [animateText] = false for a label that never changes to skip the AnimatedContent.
 */
@Composable
internal fun StatusChip(
    text: String,
    tone: Tone,
    modifier: Modifier = Modifier,
    animateText: Boolean = true,
) {
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
        val label: @Composable (String) -> Unit = {
            Text(text = it, color = content, fontSize = TypeScale.caption, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
        if (animateText) {
            AnimatedContent(text, transitionSpec = fadeThrough(), label = "chip-text") { label(it) }
        } else {
            label(text)
        }
    }
}

/**
 * Inline banner for warnings, results and recovery prompts: tone container, optional headline,
 * body text and an optional slot for actions. Changing text fades through; pass [animateText] =
 * false for text that never changes to skip the AnimatedContent.
 */
@Composable
internal fun Banner(
    text: String,
    tone: Tone,
    modifier: Modifier = Modifier,
    title: String? = null,
    gutter: Boolean = true,
    animateText: Boolean = true,
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
            val body: @Composable (String) -> Unit = {
                Text(text = it, color = onContainer, fontSize = TypeScale.subhead, lineHeight = 20.sp)
            }
            if (animateText) {
                AnimatedContent(text, transitionSpec = fadeThrough(), label = "banner-text") { body(it) }
            } else {
                body(text)
            }
            content?.invoke(this)
        }
    }
}

/** Empty / error state: 64dp round tonal icon, headline title and one subhead line. */
@Composable
internal fun EmptyState(
    title: String,
    modifier: Modifier = Modifier,
    message: String? = null,
    icon: Painter = painterResource(R.drawable.ic_space_cube),
    tone: Tone = Tone.Neutral,
) {
    StateLayout(modifier = modifier, title = title, message = message) {
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
internal fun LoadingState(title: String, modifier: Modifier = Modifier) {
    StateLayout(modifier = modifier, title = title, message = null) {
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
    }
}

/**
 * Runtime problem card shown on top of lists while the Runtime is unavailable or incompatible: a
 * full-width row with a tonal status plate that opens the Runtime page.
 */
@Composable
internal fun RuntimeStatusCard(
    runtimeReady: Boolean,
    runtimeCompatible: Boolean,
    busy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tone = when {
        busy -> Tone.Info
        runtimeReady && runtimeCompatible -> Tone.Success
        else -> Tone.Warning
    }
    val title = when {
        busy -> stringResource(R.string.environment_processing)
        runtimeReady && !runtimeCompatible ->
            stringResource(R.string.environment_version_mismatch)
        runtimeReady -> stringResource(R.string.environment_ready)
        else -> stringResource(R.string.environment_unavailable)
    }
    val description = when {
        runtimeReady && !runtimeCompatible ->
            stringResource(R.string.environment_version_mismatch_description)
        runtimeReady -> stringResource(R.string.environment_ready_description)
        else -> stringResource(R.string.environment_unavailable_description)
    }
    GroupCard(modifier = modifier, color = tone.container()) {
        ListRow(
            title = title,
            summary = description,
            icon = painterResource(R.drawable.ic_layers),
            iconTone = tone,
            leading = {
                Box(
                    Modifier
                        .size(30.dp)
                        .clip(RoundedCornerShape(Radius.icon))
                        .background(Surfaces.card.copy(alpha = .6f)),
                    contentAlignment = Alignment.Center,
                ) {
                    StatusDot(tone)
                }
            },
            onClick = onClick,
            enabled = enabled,
        )
    }
}
