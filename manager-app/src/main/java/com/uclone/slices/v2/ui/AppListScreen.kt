package com.uclone.slices.v2.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.PackageSnapshot

@Composable
internal fun AppListScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val packageByName = remember(state.packages) {
        state.packages.associateBy(PackageSnapshot::packageName)
    }
    val managedApps = remember(state.installedApps, state.packages, query) {
        buildAppSections(state.installedApps, state.packages, query).managed
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(contentPadding),
        contentPadding = PaddingValues(start = 20.dp, top = 16.dp, end = 20.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            HomeHeader(
                refreshing = state.operation is OperationUiState.Refreshing,
                onRefresh = { onIntent(UiIntent.Refresh) },
            )
        }
        item {
            RuntimeStatusCard(
                runtimeReady = state.runtimeReady,
                busy = state.busy,
                enabled = !state.busy,
                onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
            )
        }
        item {
            FilledTonalButton(
                onClick = { onIntent(UiIntent.OpenAddApps) },
                enabled = !state.busy,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp),
                contentPadding = PaddingValues(vertical = 13.dp),
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_add),
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                )
                Text(
                    text = stringResource(R.string.add_app),
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
        }

        if (!state.initialLoadComplete) {
            item { LoadingAppsMessage() }
        } else {
            item {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    shape = RoundedCornerShape(12.dp),
                    leadingIcon = {
                        Icon(
                            painter = painterResource(R.drawable.ic_search),
                            contentDescription = null,
                        )
                    },
                    placeholder = { Text(stringResource(R.string.search_configured_apps)) },
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedContainerColor = MaterialTheme.colorScheme.surface,
                        unfocusedContainerColor = MaterialTheme.colorScheme.surface,
                        focusedBorderColor = MaterialTheme.colorScheme.primary,
                        unfocusedBorderColor = MaterialTheme.colorScheme.outlineVariant,
                    ),
                )
            }
            item {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = stringResource(R.string.configured_apps),
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.weight(1f),
                    )
                    if (managedApps.isNotEmpty()) {
                        TextButton(
                            onClick = {
                                onIntent(
                                    UiIntent.SetConfiguredAccountsExpanded(
                                        !state.configuredAccountsExpanded,
                                    ),
                                )
                            },
                        ) {
                            Text(
                                stringResource(
                                    if (state.configuredAccountsExpanded) {
                                        R.string.collapse_accounts
                                    } else {
                                        R.string.expand_accounts
                                    },
                                ),
                            )
                        }
                    }
                }
            }

            if (managedApps.isEmpty()) {
                item {
                    EmptyConfiguredApps(
                        hasQuery = query.isNotBlank(),
                    )
                }
            } else {
                item {
                    ManagedAppsCard(
                        apps = managedApps,
                        packageByName = packageByName,
                        enabled = !state.busy && state.runtimeReady,
                        accountsExpanded = state.configuredAccountsExpanded,
                        onOpen = { onIntent(UiIntent.OpenPackage(it)) },
                        onActivate = { packageName, slotId ->
                            onIntent(UiIntent.QuickActivateSlot(packageName, slotId))
                        },
                        onUnenroll = { onIntent(UiIntent.RequestUnenrollApp(it)) },
                    )
                }
            }
        }
    }
}

@Composable
private fun HomeHeader(
    refreshing: Boolean,
    onRefresh: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = stringResource(R.string.spaces_title),
                style = MaterialTheme.typography.headlineMedium,
            )
            Text(
                text = stringResource(R.string.spaces_subtitle),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
        if (refreshing) {
            CircularProgressIndicator(
                modifier = Modifier
                    .padding(12.dp)
                    .size(22.dp),
                strokeWidth = 2.dp,
            )
        } else {
            IconButton(onClick = onRefresh) {
                Icon(
                    painter = painterResource(R.drawable.ic_refresh),
                    contentDescription = stringResource(R.string.refresh),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
    }
}

@Composable
private fun ManagedAppsCard(
    apps: List<InstalledApp>,
    packageByName: Map<String, PackageSnapshot>,
    enabled: Boolean,
    accountsExpanded: Boolean,
    onOpen: (String) -> Unit,
    onActivate: (String, String) -> Unit,
    onUnenroll: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        apps.forEach { app ->
            key(app.packageName) {
                val snapshot = packageByName.getValue(app.packageName)
                var menuExpanded by remember(app.packageName) { mutableStateOf(false) }
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(20.dp),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surface,
                    ),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                    elevation = CardDefaults.cardElevation(defaultElevation = 2.dp),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable(enabled = enabled) { onOpen(app.packageName) }
                            .padding(
                                start = 16.dp,
                                top = 14.dp,
                                end = 4.dp,
                                bottom = if (accountsExpanded) 0.dp else 14.dp,
                            ),
                        verticalAlignment = Alignment.Top,
                    ) {
                        AppIcon(app.label, app.icon, 58.dp)
                        Column(
                            modifier = Modifier
                                .weight(1f)
                                .padding(start = 12.dp),
                        ) {
                            Box(modifier = Modifier.fillMaxWidth()) {
                                Column(modifier = Modifier.fillMaxWidth()) {
                                    Text(
                                        text = app.label,
                                        style = MaterialTheme.typography.titleMedium.copy(
                                            lineHeight = 20.sp,
                                        ),
                                        color = if (enabled) {
                                            MaterialTheme.colorScheme.onSurface
                                        } else {
                                            MaterialTheme.colorScheme.onSurfaceVariant
                                        },
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                        modifier = Modifier.padding(end = 48.dp),
                                    )
                                    Text(
                                        text = app.packageName,
                                        style = MaterialTheme.typography.bodySmall.copy(
                                            lineHeight = 16.sp,
                                        ),
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                        modifier = Modifier.padding(end = 48.dp),
                                    )
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(top = 1.dp, end = 12.dp),
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Icon(
                                            painter = painterResource(R.drawable.ic_space_cube),
                                            contentDescription = null,
                                            tint = MaterialTheme.colorScheme.primary,
                                            modifier = Modifier.size(17.dp),
                                        )
                                        Text(
                                            text = stringResource(
                                                R.string.configured_app_summary,
                                                independentSpaceCount(snapshot),
                                                activeSlotDisplayName(snapshot),
                                            ),
                                            style = MaterialTheme.typography.bodySmall.copy(
                                                lineHeight = 16.sp,
                                            ),
                                            color = MaterialTheme.colorScheme.primary,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                            modifier = Modifier
                                                .weight(1f)
                                                .padding(start = 7.dp),
                                        )
                                    }
                                }
                                Column(modifier = Modifier.align(Alignment.TopEnd)) {
                                    IconButton(
                                        onClick = { menuExpanded = true },
                                        enabled = enabled,
                                    ) {
                                        Icon(
                                            painter = painterResource(R.drawable.ic_more_vert),
                                            contentDescription = stringResource(
                                                R.string.configured_app_actions,
                                            ),
                                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                    DropdownMenu(
                                        expanded = menuExpanded,
                                        onDismissRequest = { menuExpanded = false },
                                    ) {
                                        DropdownMenuItem(
                                            text = {
                                                Text(stringResource(R.string.unenroll_app_action))
                                            },
                                            onClick = {
                                                menuExpanded = false
                                                onUnenroll(app.packageName)
                                            },
                                        )
                                    }
                                }
                            }
                        }
                    }
                    if (accountsExpanded) {
                        FlowRow(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(start = 12.dp, top = 2.dp, end = 12.dp, bottom = 10.dp),
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            quickSwitchSlots(snapshot).forEach { slot ->
                                val selected = snapshot.activeSlot == slot.id
                                val chipShape = RoundedCornerShape(50)
                                FilterChip(
                                    selected = selected,
                                    onClick = { onActivate(app.packageName, slot.id) },
                                    enabled = enabled,
                                    modifier = Modifier.shadow(
                                        elevation = if (selected) 7.dp else 0.dp,
                                        shape = chipShape,
                                        ambientColor = SlicesWatermelon.copy(alpha = 0.28f),
                                        spotColor = SlicesWatermelon.copy(alpha = 0.28f),
                                    ),
                                    shape = chipShape,
                                    colors = FilterChipDefaults.filterChipColors(
                                        selectedContainerColor = SlicesWatermelon,
                                        selectedLabelColor = MaterialTheme.colorScheme.onError,
                                    ),
                                    leadingIcon = {
                                        Icon(
                                            painter = painterResource(
                                                if (slot.id == BASE_SLOT_ID) {
                                                    R.drawable.ic_layers
                                                } else {
                                                    R.drawable.ic_phone
                                                },
                                            ),
                                            contentDescription = null,
                                            tint = if (selected) {
                                                MaterialTheme.colorScheme.onError
                                            } else {
                                                MaterialTheme.colorScheme.onSurfaceVariant
                                            },
                                            modifier = Modifier.size(18.dp),
                                        )
                                    },
                                    label = {
                                        Text(
                                            text = if (slot.id == BASE_SLOT_ID) {
                                                stringResource(R.string.original_space)
                                            } else {
                                                spaceDisplayName(slot)
                                            },
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyConfiguredApps(
    hasQuery: Boolean,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = if (hasQuery) {
                    stringResource(R.string.no_search_results)
                } else {
                    stringResource(R.string.no_configured_apps)
                },
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                text = if (hasQuery) {
                    stringResource(R.string.try_another_search)
                } else {
                    stringResource(R.string.no_configured_apps_description)
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun LoadingAppsMessage() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(28.dp))
        Text(
            text = stringResource(R.string.loading_apps),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
