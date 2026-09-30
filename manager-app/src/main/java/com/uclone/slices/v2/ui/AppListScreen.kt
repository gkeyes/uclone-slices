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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.draw.clip
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
    var searchOpen by rememberSaveable { mutableStateOf(false) }
    val packageByName = remember(state.packages) {
        state.packages.associateBy(PackageSnapshot::packageName)
    }
    val managedApps = remember(state.installedApps, state.packages, query) {
        buildAppSections(state.installedApps, state.packages, query).managed
    }
    val runtimeHealthy = state.runtimeReady && state.runtimeCompatible
    val activating = state.operation as? OperationUiState.ActivatingSpace

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(contentPadding),
        contentPadding = PaddingValues(start = 20.dp, top = 16.dp, end = 20.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            HomeHeader(
                runtimeHealthy = runtimeHealthy,
                refreshing = state.operation is OperationUiState.Refreshing,
                busy = state.busy,
                backupEnabled = !state.busy && state.operationsAllowed,
                onStatus = { onIntent(UiIntent.OpenRuntimeStatus) },
                onSearch = {
                    searchOpen = !searchOpen
                    if (!searchOpen) query = ""
                },
                onAdd = { onIntent(UiIntent.OpenAddApps) },
                onBackup = { onIntent(UiIntent.OpenBackupRestore()) },
                onRefresh = { onIntent(UiIntent.Refresh) },
            )
        }
        // A healthy Runtime is a dot in the header; only a problem takes a whole card.
        if (!runtimeHealthy && state.initialLoadComplete) {
            item {
                RuntimeStatusCard(
                    runtimeReady = state.runtimeReady,
                    runtimeCompatible = state.runtimeCompatible,
                    busy = state.busy,
                    enabled = !state.busy,
                    onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
                )
            }
        }
        if (searchOpen || query.isNotEmpty()) {
            item {
                SlicesSearchField(
                    value = query,
                    onValueChange = { query = it },
                    label = stringResource(R.string.search_configured_apps),
                    iconRes = R.drawable.ic_search,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }

        if (!state.initialLoadComplete) {
            item { LoadingAppsMessage() }
        } else {
            item {
                Row(
                    modifier = Modifier.fillMaxWidth(),
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
                        activatingPackage = activating?.packageName,
                        activatingSlot = activating?.slotId,
                        onOpen = { onIntent(UiIntent.OpenPackage(it)) },
                        onActivate = { packageName, slotId ->
                            onIntent(UiIntent.QuickActivateSlot(packageName, slotId))
                        },
                        onSetLaunchAfterReboot = { packageName, enabled ->
                            onIntent(UiIntent.SetLaunchAfterReboot(packageName, enabled))
                        },
                        onBackup = {
                            onIntent(UiIntent.OpenBackupRestore(it, null))
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
    runtimeHealthy: Boolean,
    refreshing: Boolean,
    busy: Boolean,
    backupEnabled: Boolean,
    onStatus: () -> Unit,
    onSearch: () -> Unit,
    onAdd: () -> Unit,
    onBackup: () -> Unit,
    onRefresh: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .clip(MaterialTheme.shapes.small)
                .clickable(enabled = !busy, onClick = onStatus),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = stringResource(R.string.spaces_title),
                style = MaterialTheme.typography.headlineMedium,
                maxLines = 1,
            )
            Surface(
                modifier = Modifier
                    .padding(start = 10.dp)
                    .size(10.dp),
                shape = CircleShape,
                color = if (runtimeHealthy) SlicesSuccess else SlicesWarning,
                content = {},
            )
        }
        IconButton(onClick = onSearch) {
            Icon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = stringResource(R.string.search),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = onAdd, enabled = !busy) {
            Icon(
                painter = painterResource(R.drawable.ic_add),
                contentDescription = stringResource(R.string.add_app),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        IconButton(onClick = onBackup, enabled = backupEnabled) {
            Icon(
                painter = painterResource(R.drawable.ic_backup),
                contentDescription = stringResource(R.string.backup_restore_action),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        if (refreshing) {
            SlicesSpinner(
                size = 22.dp,
                modifier = Modifier.padding(12.dp),
            )
        } else {
            IconButton(onClick = onRefresh, enabled = !busy) {
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
    activatingPackage: String?,
    activatingSlot: String?,
    onOpen: (String) -> Unit,
    onActivate: (String, String) -> Unit,
    onSetLaunchAfterReboot: (String, Boolean) -> Unit,
    onBackup: (String) -> Unit,
    onUnenroll: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        apps.forEach { app ->
            key(app.packageName) {
                val snapshot = packageByName.getValue(app.packageName)
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
                                val ready = snapshot.bindingState == BindingState.Ready
                                val launchAfterRebootText =
                                    stringResource(R.string.launch_after_reboot_action)
                                val backupAllText =
                                    stringResource(R.string.backup_all_accounts_action)
                                val unenrollText = stringResource(R.string.unenroll_app_action)
                                SlicesOverflowMenu(
                                    items = buildList {
                                        if (ready) {
                                            add(
                                                SlicesMenuItem(
                                                    text = launchAfterRebootText,
                                                    checked = snapshot.launchAfterReboot,
                                                    onClick = {
                                                        onSetLaunchAfterReboot(
                                                            app.packageName,
                                                            !snapshot.launchAfterReboot,
                                                        )
                                                    },
                                                ),
                                            )
                                            add(
                                                SlicesMenuItem(
                                                    text = backupAllText,
                                                    onClick = { onBackup(app.packageName) },
                                                ),
                                            )
                                        }
                                        add(
                                            SlicesMenuItem(
                                                text = unenrollText,
                                                danger = true,
                                                onClick = { onUnenroll(app.packageName) },
                                            ),
                                        )
                                    },
                                    contentDescription = stringResource(
                                        R.string.configured_app_actions,
                                    ),
                                    enabled = operationsEnabled,
                                    modifier = Modifier.align(Alignment.TopEnd),
                                )
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
                                    loading = activatingPackage == app.packageName &&
                                        activatingSlot == slot.id,
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
        SlicesSpinner(size = 28.dp, strokeWidth = 3.dp)
        Text(
            text = stringResource(R.string.loading_apps),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
