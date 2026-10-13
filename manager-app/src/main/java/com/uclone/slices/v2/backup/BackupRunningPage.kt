package com.uclone.slices.v2.backup

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.Motion
import com.uclone.slices.v2.ui.SlicesLinearProgress
import com.uclone.slices.v2.ui.SlicesSpinner
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.Surfaces
import com.uclone.slices.v2.ui.Tabular
import com.uclone.slices.v2.ui.TextPrimary
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale
import com.uclone.slices.v2.ui.container
import com.uclone.slices.v2.ui.fadeThrough
import com.uclone.slices.v2.ui.onContainer
import com.uclone.slices.v2.ui.rememberReducedMotion
import com.uclone.slices.v2.ui.rollingNumber

@Composable
internal fun RunningContent(state: BackupRestoreUiState, contentPadding: PaddingValues) {
    val running = state.jobState as? BackupJobState.Running
    val reduced = rememberReducedMotion()
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Surfaces.page)
            .padding(contentPadding)
            .padding(Space.xl),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier.widthIn(max = 420.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .size(64.dp)
                    .clip(CircleShape)
                    .background(Tone.Info.container()),
                contentAlignment = Alignment.Center,
            ) {
                SlicesSpinner(size = 28.dp, color = Tone.Info.onContainer(), strokeWidth = 3.dp)
            }
            AnimatedContent(
                targetState = when (running?.kind) {
                    BackupJobKind.Backup -> stringResource(R.string.backup_running)
                    BackupJobKind.Inspect -> stringResource(R.string.inspect_running)
                    BackupJobKind.Restore -> stringResource(R.string.restore_running)
                    null -> stringResource(R.string.backup_restore_notification_text)
                },
                transitionSpec = fadeThrough(),
                label = "running-title",
                modifier = Modifier.padding(top = Space.md),
            ) { title ->
                Text(
                    text = title,
                    color = TextPrimary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center,
                )
            }
            if (running != null && running.bytesTotal > 0) {
                val fraction = (running.bytesDone.toDouble() / running.bytesTotal)
                    .coerceIn(0.0, 1.0)
                    .toFloat()
                val shown by animateFloatAsState(
                    targetValue = fraction,
                    animationSpec = if (reduced) snap() else Motion.spatial(),
                    label = "backup-progress",
                )
                SlicesLinearProgress(
                    progress = shown,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.md),
                )
                AnimatedContent(
                    targetState = stringResource(R.string.data_progress, (fraction * 100).toInt()),
                    transitionSpec = rollingNumber(),
                    label = "backup-percent",
                    modifier = Modifier.padding(top = Space.xs),
                ) { text ->
                    Text(text = text, color = TextSecondary, fontSize = TypeScale.subhead, style = Tabular)
                }
            } else if (running != null && running.total > 0) {
                AnimatedContent(
                    targetState = stringResource(R.string.restore_progress, running.completed, running.total),
                    transitionSpec = rollingNumber(),
                    label = "restore-progress",
                    modifier = Modifier.padding(top = Space.xs),
                ) { text ->
                    Text(text = text, color = TextSecondary, fontSize = TypeScale.subhead, style = Tabular)
                }
            }
            Text(
                text = stringResource(R.string.do_not_force_stop),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = Space.sm),
            )
        }
    }
}
