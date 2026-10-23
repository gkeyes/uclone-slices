package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import androidx.compose.material3.Text

@Composable
internal fun RuntimeStatusScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
    onBack: () -> Unit,
) {
    val play = rememberStaggerPlay("runtime_status")
    ManagerScreen(
        title = stringResource(R.string.runtime_status_title),
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") {
            Footnote(stringResource(R.string.runtime_status_subtitle))
        }
        item(key = "overview") {
            RuntimeOverviewCard(
                runtimeReady = state.runtimeReady,
                runtimeCompatible = state.runtimeCompatible,
                launchReady = state.launchReady,
                launchSupported = state.launchSupported,
                modifier = Modifier.staggerIn(0, play),
            )
        }
        item(key = "details_header") {
            SectionHeader(
                text = stringResource(R.string.runtime_details),
                modifier = Modifier.staggerIn(1, play),
            )
        }
        item(key = "details") {
            GroupCard(modifier = Modifier.staggerIn(2, play)) {
                ListRow(
                    title = stringResource(R.string.runtime_connection),
                    value = if (state.runtimeReady) {
                        stringResource(R.string.connected)
                    } else {
                        stringResource(R.string.disconnected)
                    },
                    valueColor = if (state.runtimeReady) {
                        Tone.Success.onContainer()
                    } else {
                        Tone.Warning.onContainer()
                    },
                )
                ListRow(
                    title = stringResource(R.string.runtime_version),
                    value = state.buildId.takeIf(String::isNotBlank)
                        ?: stringResource(R.string.runtime_version_unverified),
                    valueColor = TextPrimary,
                )
                ListRow(
                    title = stringResource(R.string.launch_support_title),
                    value = stringResource(
                        when {
                            state.launchReady -> R.string.launch_support_ready
                            !state.launchSupported -> R.string.launch_support_unsupported
                            else -> R.string.launch_support_pending
                        },
                    ),
                    valueColor = if (state.launchReady) Tone.Success.onContainer() else Tone.Warning.onContainer(),
                )
            }
        }
        item(key = "actions") {
            Column(
                modifier = Modifier
                    .staggerIn(3, play)
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter, vertical = Space.xs),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                AppButton(
                    text = if (state.runtimeReady) {
                        stringResource(R.string.refresh_status)
                    } else {
                        stringResource(R.string.reconnect)
                    },
                    onClick = { onIntent(UiIntent.Refresh) },
                    enabled = !state.busy,
                    loading = state.operation is OperationUiState.Refreshing,
                    icon = painterResource(R.drawable.ic_refresh),
                    kind = if (state.operationsAllowed) ButtonKind.Primary else ButtonKind.Secondary,
                    modifier = Modifier.fillMaxWidth(),
                )
                AppButton(
                    text = stringResource(R.string.restart_runtime),
                    onClick = { onIntent(UiIntent.RestartRuntime) },
                    enabled = !state.busy,
                    kind = if (state.operationsAllowed) ButtonKind.Secondary else ButtonKind.Primary,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        item(key = "restart_hint") {
            Footnote(stringResource(R.string.restart_runtime_hint))
        }
        if (state.runtimeReady && state.runtimeCompatible && !state.launchReady) {
            item(key = "launch_support_hint") {
                Footnote(
                    stringResource(
                        if (state.launchSupported) R.string.launch_support_hint else R.string.launch_support_unsupported_hint,
                    ),
                )
            }
        }
        item(key = "log_header") {
            SectionHeader(
                text = stringResource(R.string.runtime_log),
                trailing = {
                    AppButton(
                        text = stringResource(R.string.load_runtime_log),
                        onClick = { onIntent(UiIntent.LoadRuntimeLog) },
                        kind = ButtonKind.Text,
                        compact = true,
                    )
                },
            )
        }
        val log = state.runtimeLog
        if (log != null) {
            item(key = "log") {
                GroupCard(padded = true, modifier = Modifier.animateItem()) {
                    AnimatedContent(log, transitionSpec = fadeThrough(), label = "runtime-log") { text ->
                        Text(
                            text = text.ifBlank { stringResource(R.string.runtime_log_empty) },
                            color = TextSecondary,
                            fontSize = TypeScale.caption,
                            lineHeight = 17.sp,
                            fontFamily = FontFamily.Monospace,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun RuntimeOverviewCard(
    runtimeReady: Boolean,
    runtimeCompatible: Boolean,
    launchReady: Boolean,
    launchSupported: Boolean,
    modifier: Modifier = Modifier,
) {
    val ready = runtimeReady && runtimeCompatible && launchReady
    val tone = if (ready) Tone.Success else Tone.Warning
    val title = when {
        runtimeReady && !runtimeCompatible -> stringResource(R.string.environment_version_mismatch)
        runtimeReady && runtimeCompatible && !launchReady -> stringResource(
            if (launchSupported) R.string.launch_support_pending else R.string.launch_support_unsupported,
        )
        runtimeReady -> stringResource(R.string.environment_ready)
        else -> stringResource(R.string.environment_unavailable)
    }
    GroupCard(modifier = modifier, padded = true) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            IconPlate(painterResource(R.drawable.ic_layers), tone, size = 44.dp)
            AnimatedContent(
                targetState = title,
                transitionSpec = fadeThrough(),
                label = "runtime-title",
                modifier = Modifier.weight(1f),
            ) { shown ->
                Text(
                    text = shown,
                    color = TextPrimary,
                    fontSize = TypeScale.title,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            StatusChip(
                text = if (runtimeReady) {
                    stringResource(R.string.connected)
                } else {
                    stringResource(R.string.disconnected)
                },
                tone = tone,
            )
        }
        AnimatedContent(
            targetState = when {
                runtimeReady && !runtimeCompatible ->
                    stringResource(R.string.runtime_version_mismatch_long)
                runtimeReady -> stringResource(R.string.runtime_ready_long)
                else -> stringResource(R.string.runtime_unavailable_long)
            },
            transitionSpec = fadeThrough(),
            label = "runtime-overview",
        ) { text ->
            Text(
                text = text,
                color = TextSecondary,
                fontSize = TypeScale.subhead,
                lineHeight = 20.sp,
                modifier = Modifier.padding(top = Space.sm),
            )
        }
    }
}
