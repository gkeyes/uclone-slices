package com.uclone.slices.v2.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.PackageSnapshot
import androidx.compose.material3.Icon
import androidx.compose.material3.Text

@Composable
internal fun AppListScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
    uiPreferences: ManagerUiPreferences,
) {
    var query by rememberSaveable { mutableStateOf("") }
    var searchOpen by rememberSaveable { mutableStateOf(false) }
    val packageByName = remember(state.packages) {
        state.packages.associateBy(PackageSnapshot::packageName)
    }
    // Manual home order: UI-only, read from local preferences and applied to the list.
    var savedOrder by remember { mutableStateOf(uiPreferences.homeAppOrder) }
    val managedApps = remember(state.installedApps, state.packages, query, savedOrder) {
        HomeOrder.sort(
            buildAppSections(state.installedApps, state.packages, query).managed,
            savedOrder,
        ) { it.packageName }
    }
    var reordering by rememberSaveable { mutableStateOf(false) }
    var draft by remember { mutableStateOf(emptyList<String>()) }
    val currentKeys = managedApps.map(InstalledApp::packageName)
    if (reordering && draft.isEmpty() && currentKeys.isNotEmpty()) {
        // Restored after process death or rotation: start from the saved order again.
        draft = currentKeys
    }
    val draftKeys = HomeOrder.merge(draft, currentKeys)
    val appByName = managedApps.associateBy(InstalledApp::packageName)
    val canReorder = state.initialLoadComplete && !searchOpen && query.isEmpty() && managedApps.size > 1
    val finishReorder: (Boolean) -> Unit = { save ->
        if (save) {
            val merged = HomeOrder.merge(draft, currentKeys)
            uiPreferences.homeAppOrder = merged
            savedOrder = merged
        }
        reordering = false
        draft = emptyList()
    }
    BackHandler(enabled = reordering) { finishReorder(false) }

    val runtimeHealthy = state.runtimeReady && state.runtimeCompatible
    val activating = state.operation as? OperationUiState.ActivatingSpace
    val refreshing = state.operation is OperationUiState.Refreshing
    val play = rememberStaggerPlay("home")
    val enabled = !state.busy && state.runtimeReady
    val operationsEnabled = !state.busy && state.operationsAllowed
    val reduced = rememberReducedMotion()
    val listState = rememberLazyListState()
    val reorderState = rememberHomeReorderState(listState) { from, to ->
        draft = HomeOrder.moveTo(HomeOrder.merge(draft, currentKeys), from, to)
    }

    ManagerScreen(
        title = stringResource(R.string.spaces_title),
        contentPadding = contentPadding,
        listState = listState,
        titleAccessory = {
            RuntimeHealthDot(
                healthy = runtimeHealthy,
                enabled = !state.busy,
                onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
            )
        },
        actions = {
            if (reordering) {
                AppButton(
                    text = stringResource(R.string.cancel),
                    onClick = { finishReorder(false) },
                    kind = ButtonKind.Text,
                    compact = true,
                )
                AppButton(
                    text = stringResource(R.string.done),
                    onClick = { finishReorder(true) },
                    compact = true,
                    modifier = Modifier.padding(end = Space.xs),
                )
            } else {
                HomeActions(
                    state = state,
                    searchOpen = searchOpen,
                    canReorder = canReorder,
                    refreshing = refreshing,
                    onToggleSearch = {
                        searchOpen = !searchOpen
                        if (!searchOpen) query = ""
                    },
                    onReorder = {
                        draft = currentKeys
                        reordering = true
                    },
                    onIntent = onIntent,
                )
            }
        },
    ) {
        if (reordering) {
            item(key = "reorder_hint") {
                Footnote(
                    text = stringResource(R.string.reorder_hint),
                    modifier = Modifier
                        .animateItem()
                        .padding(vertical = Space.xs),
                )
            }
            draftKeys.forEachIndexed { index, packageName ->
                val app = appByName[packageName] ?: return@forEachIndexed
                val snapshot = packageByName[packageName]
                item(key = reorderItemKey(packageName)) {
                    ReorderRow(
                        app = app,
                        accountCount = snapshot?.let(::independentSpaceCount) ?: 0,
                        index = index,
                        lastIndex = draftKeys.lastIndex,
                        state = reorderState,
                        onMoveUp = { draft = HomeOrder.moveUp(HomeOrder.merge(draft, currentKeys), packageName) },
                        onMoveDown = { draft = HomeOrder.moveDown(HomeOrder.merge(draft, currentKeys), packageName) },
                        modifier = Modifier.animateItem(
                            placementSpec = if (reduced || reorderState.draggingKey == packageName) {
                                null
                            } else {
                                Motion.offset
                            },
                        ),
                    )
                }
            }
            return@ManagerScreen
        }
        // A healthy Runtime is a dot next to the title; only a problem takes a whole card.
        if (!runtimeHealthy && state.initialLoadComplete) {
            item(key = "runtime_status") {
                RuntimeStatusCard(
                    runtimeReady = state.runtimeReady,
                    runtimeCompatible = state.runtimeCompatible,
                    busy = state.busy,
                    enabled = !state.busy,
                    onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
                    modifier = Modifier.animateItem(),
                )
            }
        }
        if (searchOpen || query.isNotEmpty()) {
            item(key = "search") {
                SlicesSearchField(
                    value = query,
                    onValueChange = { query = it },
                    label = stringResource(R.string.search_configured_apps),
                    clearDescription = stringResource(R.string.cancel),
                    modifier = Modifier
                        .animateItem()
                        .fillMaxWidth()
                        .padding(horizontal = Space.gutter),
                )
            }
        }

        if (!state.initialLoadComplete) {
            item(key = "loading") {
                LoadingState(
                    title = stringResource(R.string.loading_apps),
                    modifier = Modifier.animateItem(),
                )
            }
        } else {
            item(key = "configured_header") {
                SectionHeader(
                    text = stringResource(R.string.configured_apps),
                    modifier = Modifier
                        .animateItem()
                        .staggerIn(0, play),
                    trailing = if (managedApps.isNotEmpty()) {
                        {
                            ExpandToggle(
                                expanded = state.configuredAccountsExpanded,
                                onToggle = {
                                    onIntent(
                                        UiIntent.SetConfiguredAccountsExpanded(
                                            !state.configuredAccountsExpanded,
                                        ),
                                    )
                                },
                            )
                        }
                    } else {
                        null
                    },
                )
            }

            if (managedApps.isEmpty()) {
                item(key = "empty") {
                    GroupCard(modifier = Modifier.animateItem()) {
                        EmptyState(
                            title = if (query.isNotBlank()) {
                                stringResource(R.string.no_search_results)
                            } else {
                                stringResource(R.string.no_configured_apps)
                            },
                            message = if (query.isNotBlank()) {
                                stringResource(R.string.try_another_search)
                            } else {
                                stringResource(R.string.no_configured_apps_description)
                            },
                            icon = painterResource(
                                if (query.isNotBlank()) R.drawable.ic_search else R.drawable.ic_space_cube,
                            ),
                        )
                    }
                }
            } else {
                managedApps.forEachIndexed { index, app ->
                    val snapshot = packageByName.getValue(app.packageName)
                    item(key = "app:${app.packageName}") {
                        ManagedAppCard(
                            app = app,
                            snapshot = snapshot,
                            enabled = enabled,
                            operationsEnabled = operationsEnabled,
                            accountsExpanded = state.configuredAccountsExpanded,
                            activatingSlot = activating
                                ?.takeIf { it.packageName == app.packageName }
                                ?.slotId,
                            onOpen = { onIntent(UiIntent.OpenPackage(app.packageName)) },
                            onActivate = { slotId ->
                                onIntent(UiIntent.QuickActivateSlot(app.packageName, slotId))
                            },
                            onSetLaunchAfterReboot = { value ->
                                onIntent(UiIntent.SetLaunchAfterReboot(app.packageName, value))
                            },
                            onBackup = {
                                onIntent(UiIntent.OpenBackupRestore(app.packageName, null))
                            },
                            onUnenroll = { onIntent(UiIntent.RequestUnenrollApp(app.packageName)) },
                            modifier = Modifier
                                .animateItem()
                                .staggerIn(index + 1, play),
                        )
                    }
                }
            }
        }
    }
}

/** Normal-mode title bar actions: search, sort, add, backup, refresh. */
@Composable
private fun RowScope.HomeActions(
    state: SlotsUiState,
    searchOpen: Boolean,
    canReorder: Boolean,
    refreshing: Boolean,
    onToggleSearch: () -> Unit,
    onReorder: () -> Unit,
    onIntent: (UiIntent) -> Unit,
) {
    TopBarIconButton(
        painter = painterResource(R.drawable.ic_search),
        contentDescription = stringResource(R.string.search),
        onClick = onToggleSearch,
        tint = if (searchOpen) Accent else TextSecondary,
    )
    if (canReorder) {
        TopBarIconButton(
            painter = painterResource(R.drawable.ic_sort),
            contentDescription = stringResource(R.string.sort_apps),
            onClick = onReorder,
            enabled = !state.busy,
            tint = TextSecondary,
        )
    }
    TopBarIconButton(
        painter = painterResource(R.drawable.ic_add),
        contentDescription = stringResource(R.string.add_app),
        onClick = { onIntent(UiIntent.OpenAddApps) },
        enabled = !state.busy,
    )
    TopBarIconButton(
        painter = painterResource(R.drawable.ic_backup),
        contentDescription = stringResource(R.string.backup_restore_action),
        onClick = { onIntent(UiIntent.OpenBackupRestore()) },
        enabled = !state.busy && state.operationsAllowed,
    )
    if (refreshing) {
        Box(Modifier.size(Space.glassButton), contentAlignment = Alignment.Center) {
            SlicesSpinner(size = 22.dp)
        }
    } else {
        TopBarIconButton(
            painter = painterResource(R.drawable.ic_refresh),
            contentDescription = stringResource(R.string.refresh),
            onClick = { onIntent(UiIntent.Refresh) },
            enabled = !state.busy,
        )
    }
}

/** Runtime health next to the large title: a tonal dot with a 48dp touch target. */
@Composable
private fun RuntimeHealthDot(
    healthy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val description = stringResource(R.string.runtime_status_title)
    Box(
        modifier = Modifier
            .size(Space.touchTarget)
            .pressFeedback(enabled, Motion.PressedScale, CircleShape)
            .clip(CircleShape)
            .clickable(
                enabled = enabled,
                role = Role.Button,
                onClickLabel = description,
                interactionSource = null,
                indication = null,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        StatusDot(if (healthy) Tone.Success else Tone.Warning, size = 12.dp)
    }
}

/** Expand / collapse all account chips; the chevron rotates 180 degrees on the spatial spring. */
@Composable
private fun ExpandToggle(expanded: Boolean, onToggle: () -> Unit) {
    val reduced = rememberReducedMotion()
    val rotation by animateFloatAsState(
        targetValue = if (expanded) 270f else 90f,
        animationSpec = if (reduced) snap() else Motion.spatial(),
        label = "expand-chevron",
    )
    Row(
        modifier = Modifier
            .pressFeedback(true, Motion.PressedScale, androidx.compose.foundation.shape.RoundedCornerShape(Radius.chip))
            .clip(androidx.compose.foundation.shape.RoundedCornerShape(Radius.chip))
            .clickable(
                role = Role.Button,
                interactionSource = null,
                indication = null,
                onClick = onToggle,
            )
            .padding(horizontal = Space.sm, vertical = Space.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.xxs),
    ) {
        Text(
            text = stringResource(if (expanded) R.string.collapse_accounts else R.string.expand_accounts),
            color = Accent,
            fontSize = TypeScale.sectionHeader,
            fontWeight = FontWeight.Medium,
        )
        Icon(
            painter = painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = Accent,
            modifier = Modifier
                .size(16.dp)
                .graphicsLayer { rotationZ = rotation },
        )
    }
}

@Composable
private fun ManagedAppCard(
    app: InstalledApp,
    snapshot: PackageSnapshot,
    enabled: Boolean,
    operationsEnabled: Boolean,
    accountsExpanded: Boolean,
    activatingSlot: String?,
    onOpen: () -> Unit,
    onActivate: (String) -> Unit,
    onSetLaunchAfterReboot: (Boolean) -> Unit,
    onBackup: () -> Unit,
    onUnenroll: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduced = rememberReducedMotion()
    val ready = snapshot.bindingState == BindingState.Ready
    GroupCard(
        modifier = modifier,
        enabled = enabled,
        onClick = onOpen,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = Space.md, top = Space.sm, end = Space.xxs, bottom = Space.sm),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            AppIcon(app.label, app.icon, 52.dp)
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(
                    text = app.label,
                    color = if (enabled) TextPrimary else TextSecondary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = app.packageName,
                    color = TextTertiary,
                    fontSize = TypeScale.caption,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_space_cube),
                        contentDescription = null,
                        tint = Accent,
                        modifier = Modifier.size(15.dp),
                    )
                    androidx.compose.animation.AnimatedContent(
                        targetState = stringResource(
                            R.string.configured_app_summary,
                            independentSpaceCount(snapshot),
                            activeSlotDisplayName(snapshot),
                        ),
                        transitionSpec = fadeThrough(),
                        label = "app-summary",
                    ) { summary ->
                        Text(
                            text = summary,
                            color = Accent,
                            fontSize = TypeScale.caption,
                            style = Tabular,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                if (!ready) {
                    StatusChip(
                        text = stringResource(
                            when (snapshot.bindingState) {
                                BindingState.LegacyConfirmationRequired ->
                                    R.string.binding_confirmation_required
                                BindingState.LegacyUnbound,
                                BindingState.RebindRequired,
                                -> R.string.binding_recovery_pending
                                BindingState.Ready -> R.string.binding_ready
                            },
                        ),
                        tone = Tone.Warning,
                        modifier = Modifier.padding(top = Space.xxs),
                    )
                }
            }
            val launchAfterRebootText = stringResource(R.string.launch_after_reboot_action)
            val backupAllText = stringResource(R.string.backup_all_accounts_action)
            val unenrollText = stringResource(R.string.unenroll_app_action)
            SlicesOverflowMenu(
                items = buildList {
                    if (ready) {
                        add(
                            SlicesMenuItem(
                                text = launchAfterRebootText,
                                checked = snapshot.launchAfterReboot,
                                onClick = { onSetLaunchAfterReboot(!snapshot.launchAfterReboot) },
                            ),
                        )
                        add(SlicesMenuItem(text = backupAllText, onClick = onBackup))
                    }
                    add(SlicesMenuItem(text = unenrollText, danger = true, onClick = onUnenroll))
                },
                contentDescription = stringResource(R.string.configured_app_actions),
                enabled = operationsEnabled,
                modifier = Modifier.align(Alignment.Top),
            )
        }
        AnimatedVisibility(
            visible = accountsExpanded,
            enter = if (reduced) {
                androidx.compose.animation.EnterTransition.None
            } else {
                expandVertically(Motion.size) + fadeIn(Motion.effects())
            },
            exit = if (reduced) {
                androidx.compose.animation.ExitTransition.None
            } else {
                shrinkVertically(Motion.size) + fadeOut(Motion.effects())
            },
        ) {
            FlowRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = Space.md, end = Space.md, bottom = Space.md),
                horizontalArrangement = Arrangement.spacedBy(Space.xs),
                verticalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                quickSwitchSlots(snapshot).forEach { slot ->
                    SlicesSlotButton(
                        text = if (slot.id == BASE_SLOT_ID) {
                            stringResource(R.string.original_space)
                        } else {
                            spaceDisplayName(slot)
                        },
                        icon = painterResource(
                            if (slot.id == BASE_SLOT_ID) R.drawable.ic_layers else R.drawable.ic_phone,
                        ),
                        selected = snapshot.activeSlot == slot.id,
                        onClick = { onActivate(slot.id) },
                        enabled = operationsEnabled && ready,
                        loading = activatingSlot == slot.id,
                    )
                }
            }
        }
    }
}
