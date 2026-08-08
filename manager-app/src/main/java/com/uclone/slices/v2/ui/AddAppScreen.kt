package com.uclone.slices.v2.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import top.yukonga.miuix.kmp.basic.IconButton
import top.yukonga.miuix.kmp.extra.SuperArrow

@Composable
internal fun AddAppScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val availableApps = remember(state.installedApps, state.packages, query) {
        buildAppSections(state.installedApps, state.packages, query).other
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(contentPadding),
        contentPadding = PaddingValues(start = 20.dp, top = 12.dp, end = 20.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            ScreenHeader(
                title = stringResource(R.string.add_app),
                subtitle = stringResource(R.string.add_app_subtitle),
                onBack = { onIntent(UiIntent.NavigateBack) },
            )
        }
        if (!state.runtimeReady) {
            item {
                RuntimeStatusCard(
                    runtimeReady = false,
                    busy = state.busy,
                    enabled = !state.busy,
                    onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
                )
            }
        }
        item {
            SlicesSearchField(
                value = query,
                onValueChange = { query = it },
                label = stringResource(R.string.search_available_apps),
                iconRes = R.drawable.ic_search,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Text(
                text = stringResource(R.string.available_apps),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
        if (availableApps.isEmpty()) {
            item {
                EmptyAvailableApps(hasQuery = query.isNotBlank())
            }
        } else {
            item {
                AvailableAppsCard(
                    apps = availableApps,
                    enabled = state.runtimeReady && !state.busy,
                    onConfigure = { onIntent(UiIntent.RequestConfigureApp(it)) },
                )
            }
        }
    }
}

@Composable
internal fun ScreenHeader(
    title: String,
    subtitle: String? = null,
    onBack: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack) {
            Icon(
                painter = painterResource(R.drawable.ic_arrow_back),
                contentDescription = stringResource(R.string.back),
            )
        }
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(start = 4.dp),
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            subtitle?.let {
                Text(
                    text = it,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun AvailableAppsCard(
    apps: List<InstalledApp>,
    enabled: Boolean,
    onConfigure: (String) -> Unit,
) {
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
        cornerRadius = 18.dp,
    ) {
        apps.forEachIndexed { index, app ->
            SuperArrow(
                title = app.label,
                summary = "${app.packageName}\n${stringResource(R.string.not_configured)}",
                leftAction = {
                    Row {
                        AppIcon(app.label, app.icon, 48.dp)
                        Spacer(Modifier.width(14.dp))
                    }
                },
                enabled = enabled,
                onClick = { onConfigure(app.packageName) },
                modifier = Modifier.fillMaxWidth(),
                insideMargin = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
            )
            if (index < apps.lastIndex) {
                HorizontalDivider(
                    modifier = Modifier.padding(start = 80.dp),
                    color = MaterialTheme.colorScheme.outlineVariant,
                )
            }
        }
    }
}

@Composable
private fun EmptyAvailableApps(hasQuery: Boolean) {
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
        cornerRadius = 18.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 36.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = if (hasQuery) {
                    stringResource(R.string.no_search_results)
                } else {
                    stringResource(R.string.no_available_apps)
                },
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                text = if (hasQuery) {
                    stringResource(R.string.try_another_search)
                } else {
                    stringResource(R.string.no_available_apps_description)
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
