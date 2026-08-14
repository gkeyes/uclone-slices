package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.PackageSnapshot
import top.yukonga.miuix.kmp.basic.IconButton

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
                runtimeCompatible = state.runtimeCompatible,
                busy = state.busy,
                enabled = !state.busy,
                onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
            )
        }
        item {
            SlicesActionButton(
                text = stringResource(R.string.add_app),
                onClick = { onIntent(UiIntent.OpenAddApps) },
                enabled = !state.busy,
                modifier = Modifier.fillMaxWidth(),
                primary = false,
                icon = painterResource(R.drawable.ic_add),
            )
        }

        if (!state.initialLoadComplete) {
            item { LoadingAppsMessage() }
        } else {
            item {
                SlicesSearchField(
                    value = query,
                    onValueChange = { query = it },
                    label = stringResource(R.string.search_configured_apps),
                    iconRes = R.drawable.ic_search,
                    modifier = Modifier.fillMaxWidth(),
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
                        SlicesHeaderButton(
                            text = stringResource(
                                if (state.configuredAccountsExpanded) {
                                    R.string.collapse_accounts
                                } else {
                                    R.string.expand_accounts
                                },
                            ),
                            onClick = {
                                onIntent(
                                    UiIntent.SetConfiguredAccountsExpanded(
                                        !state.configuredAccountsExpanded,
                                    ),
                                )
                            },
                        )
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
                        operationsEnabled = !state.busy && state.operationsAllowed,
                        accountsExpanded = state.configuredAccountsExpanded,
                        onOpen = { onIntent(UiIntent.OpenPackage(it)) },
                        onActivate = { packageName, slotId ->
                            onIntent(UiIntent.QuickActivateSlot(packageName, slotId))
                        },
                        onSetLaunchAfterReboot = { packageName, enabled ->
                            onIntent(UiIntent.SetLaunchAfterReboot(packageName, enabled))
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
    operationsEnabled: Boolean,
    accountsExpanded: Boolean,
    onOpen: (String) -> Unit,
    onActivate: (String, String) -> Unit,
    onSetLaunchAfterReboot: (String, Boolean) -> Unit,
    onUnenroll: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        apps.forEach { app ->
            key(app.packageName) {
                val snapshot = packageByName.getValue(app.packageName)
                var menuExpanded by remember(app.packageName) { mutableStateOf(false) }
                SlicesPanel(
                    modifier = Modifier.fillMaxWidth(),
                    enabled = enabled,
                    onClick = { onOpen(app.packageName) },
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
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
                                    if (snapshot.bindingState != BindingState.Ready) {
                                        Text(
                                            text = stringResource(
                                                when (snapshot.bindingState) {
                                                    BindingState.LegacyConfirmationRequired ->
                                                        R.string.binding_confirmation_required
                                                    BindingState.LegacyUnbound,
                                                    BindingState.RebindRequired,
                                                    -> R.string.binding_recovery_pending
                                                    BindingState.Ready ->
                                                        R.string.binding_ready
                                                },
                                            ),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = SlicesWarning,
                                            modifier = Modifier.padding(top = 2.dp),
                                        )
                                    }
                                }
                                Column(modifier = Modifier.align(Alignment.TopEnd)) {
                                    IconButton(
                                        onClick = { menuExpanded = true },
                                        enabled = operationsEnabled,
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
                                                Text(
                                                    stringResource(
                                                        R.string.launch_after_reboot_action,
                                                    ),
                                                )
                                            },
                                            trailingIcon = {
                                                Switch(
                                                    checked = snapshot.launchAfterReboot,
                                                    onCheckedChange = null,
                                                    enabled = operationsEnabled &&
                                                        snapshot.bindingState == BindingState.Ready,
                                                )
                                            },
                                            enabled = operationsEnabled &&
                                                snapshot.bindingState == BindingState.Ready,
                                            onClick = {
                                                menuExpanded = false
                                                onSetLaunchAfterReboot(
                                                    app.packageName,
                                                    !snapshot.launchAfterReboot,
                                                )
                                            },
                                        )
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
                    AnimatedVisibility(
                        visible = accountsExpanded,
                        enter = expandVertically() + fadeIn(),
                        exit = shrinkVertically() + fadeOut(),
                    ) {
                        FlowRow(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(start = 12.dp, top = 14.dp, end = 12.dp, bottom = 10.dp),
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            quickSwitchSlots(snapshot).forEach { slot ->
                                val selected = snapshot.activeSlot == slot.id
                                SlicesSlotButton(
                                    text = if (slot.id == BASE_SLOT_ID) {
                                        stringResource(R.string.original_space)
                                    } else {
                                        spaceDisplayName(slot)
                                    },
                                    icon = painterResource(
                                        if (slot.id == BASE_SLOT_ID) {
                                            R.drawable.ic_layers
                                        } else {
                                            R.drawable.ic_phone
                                        },
                                    ),
                                    selected = selected,
                                    onClick = { onActivate(app.packageName, slot.id) },
                                    enabled = operationsEnabled &&
                                        snapshot.bindingState == BindingState.Ready,
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
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
        cornerRadius = 18.dp,
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
