package com.uclone.slices.v2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R

@Composable
internal fun RuntimeStatusScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
) {
    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(contentPadding),
        contentPadding = PaddingValues(start = 20.dp, top = 12.dp, end = 20.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            ScreenHeader(
                title = stringResource(R.string.runtime_status_title),
                subtitle = stringResource(R.string.runtime_status_subtitle),
                onBack = { onIntent(UiIntent.NavigateBack) },
            )
        }
        item {
            RuntimeOverviewCard(
                runtimeReady = state.runtimeReady,
                runtimeCompatible = state.runtimeCompatible,
            )
        }
        item {
            Text(
                text = stringResource(R.string.runtime_details),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
        item {
            RuntimeDetailsCard(state)
        }
        item {
            SlicesActionButton(
                text = if (state.runtimeReady) {
                    stringResource(R.string.refresh_status)
                } else {
                    stringResource(R.string.reconnect)
                },
                onClick = { onIntent(UiIntent.Refresh) },
                enabled = !state.busy,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun RuntimeOverviewCard(runtimeReady: Boolean, runtimeCompatible: Boolean) {
    val ready = runtimeReady && runtimeCompatible
    val accent = if (ready) SlicesSuccess else SlicesWarning
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(20.dp),
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Surface(
                modifier = Modifier.size(12.dp),
                shape = RoundedCornerShape(6.dp),
                color = accent,
                content = {},
            )
            Column {
                Text(
                    text = when {
                        runtimeReady && !runtimeCompatible ->
                            stringResource(R.string.environment_version_mismatch)
                        runtimeReady -> stringResource(R.string.environment_ready)
                        else -> stringResource(R.string.environment_unavailable)
                    },
                    style = MaterialTheme.typography.titleLarge,
                )
                Text(
                    text = when {
                        runtimeReady && !runtimeCompatible ->
                            stringResource(R.string.runtime_version_mismatch_long)
                        runtimeReady -> stringResource(R.string.runtime_ready_long)
                        else -> stringResource(R.string.runtime_unavailable_long)
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun RuntimeDetailsCard(state: SlotsUiState) {
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
        cornerRadius = 18.dp,
    ) {
        RuntimeDetailRow(
            label = stringResource(R.string.runtime_connection),
            value = if (state.runtimeReady) {
                stringResource(R.string.connected)
            } else {
                stringResource(R.string.disconnected)
            },
            valueColor = if (state.runtimeReady) SlicesSuccess else SlicesWarning,
        )
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outlineVariant,
        )
        RuntimeDetailRow(
            label = stringResource(R.string.runtime_version),
            value = state.buildId.takeIf(String::isNotBlank)
                ?: stringResource(R.string.unavailable),
            valueColor = MaterialTheme.colorScheme.onSurface,
        )
    }
}

@Composable
private fun RuntimeDetailRow(
    label: String,
    value: String,
    valueColor: androidx.compose.ui.graphics.Color,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 15.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            color = valueColor,
        )
    }
}
