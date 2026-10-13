package com.uclone.slices.v2.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.backup.BackupRestoreScreen
import com.uclone.slices.v2.backup.BackupRestoreViewModel
import com.uclone.slices.v2.backup.BackupUiIntent

@Composable
internal fun SlicesScreen(
    viewModel: SlotsViewModel,
    backupViewModelFactory: ViewModelProvider.Factory,
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // Remembered so the pages get stable callbacks. `state` is a delegated property of a
    // remembered State, so every read inside them sees the latest value.
    val onIntent: (UiIntent) -> Unit = remember(viewModel) {
        { intent: UiIntent -> viewModel.onIntent(intent) }
    }
    val navController = rememberNavController()
    val hostLifecycle = LocalLifecycleOwner.current
    // Moves the view model asks for, and what is on top reported back to it.
    LaunchedEffect(viewModel, navController) {
        viewModel.navigation.collect { navController.follow(it) }
    }
    LaunchedEffect(viewModel, navController) {
        navController.currentBackStackEntryFlow.collect { entry ->
            viewModel.onIntent(UiIntent.DestinationChanged(entry.managerDestination()))
        }
    }
    val backStackEntry by navController.currentBackStackEntryAsState()
    val atSpaces = backStackEntry?.destination?.hasRoute<SpacesRoute>() != false
    // Pages above the spaces list go back one page, never while an operation runs. Backup and
    // restore handles its own back (it leaves its job state first and asks for a refresh).
    val navigateBack: () -> Unit = remember(navController) {
        { if (!state.busy) navController.popPage() }
    }

    val reduced = rememberReducedMotion()
    // The pages are recorded here; the sheets, the operation alert and the top notice drawn above
    // them refract that recording. They sit outside it, so glass never samples itself.
    val pageBackdrop = rememberGlassBackdrop()
    val systemBars = WindowInsets.systemBars.union(WindowInsets.displayCutout).asPaddingValues()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Surfaces.page),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .glassSource(pageBackdrop),
        ) {
            NavHost(
                navController = navController,
                startDestination = SpacesRoute,
                modifier = Modifier.fillMaxSize(),
                enterTransition = { iosPush(forward = true, reduced = reduced).targetContentEnter },
                exitTransition = { iosPush(forward = true, reduced = reduced).initialContentExit },
                popEnterTransition = {
                    iosPush(forward = false, reduced = reduced).targetContentEnter
                },
                popExitTransition = {
                    iosPush(forward = false, reduced = reduced).initialContentExit
                },
            ) {
                composable<SpacesRoute> { entry ->
                    HostLifecycle(hostLifecycle) {
                        RefreshAfterBackup(entry) { onIntent(UiIntent.Refresh) }
                        AppListScreen(
                            state = state,
                            contentPadding = systemBars,
                            onIntent = onIntent,
                        )
                    }
                }
                composable<AddAppRoute> {
                    HostLifecycle(hostLifecycle) {
                        AddAppScreen(
                            state = state,
                            contentPadding = systemBars,
                            onIntent = onIntent,
                            onBack = navigateBack,
                        )
                    }
                }
                composable<PackageDetailsRoute> { entry ->
                    HostLifecycle(hostLifecycle) {
                        RefreshAfterBackup(entry) { onIntent(UiIntent.Refresh) }
                        // Kept while the page slides out after the view model has let it go.
                        val selected = rememberRetained(state.selected)
                        if (selected != null) {
                            PackageDetailScreen(
                                state = state,
                                packageSnapshot = selected,
                                installedApp = state.installedApps.firstOrNull {
                                    it.packageName == selected.packageName
                                },
                                contentPadding = systemBars,
                                onIntent = onIntent,
                                onBack = navigateBack,
                            )
                        }
                    }
                }
                composable<BackupRestoreRoute> { entry ->
                    HostLifecycle(hostLifecycle) {
                        BackupRestoreDestination(
                            route = entry.toRoute<BackupRestoreRoute>(),
                            // False while the page slides out: back then belongs to the page below.
                            active = backStackEntry?.id == entry.id,
                            factory = backupViewModelFactory,
                            slotsState = state,
                            contentPadding = systemBars,
                            onLeave = { navController.returnFromBackup() },
                        )
                    }
                }
                composable<RuntimeStatusRoute> {
                    HostLifecycle(hostLifecycle) {
                        RuntimeStatusScreen(
                            state = state,
                            contentPadding = systemBars,
                            onIntent = onIntent,
                            onBack = navigateBack,
                        )
                    }
                }
            }
        }
        // Registered after the NavHost's own handler, so it decides: no page change while an
        // operation runs. Pages and sheets that handle back themselves (backup and restore,
        // reordering, sheets) are composed under the same lifecycle later still and outrank it.
        BackHandler(enabled = !atSpaces, onBack = navigateBack)

        CompositionLocalProvider(LocalGlassBackdrop provides pageBackdrop) {
            RootOverlays(
                state = state,
                onIntent = onIntent,
                noticeModifier = Modifier
                    .align(Alignment.TopCenter)
                    .zIndex(10f),
            )
        }
    }
}

/**
 * Backup and restore with its own view model, scoped to this back-stack entry. Back (system or
 * the title bar) leaves its job state and returns to the page it was opened from, which then
 * refreshes; never while a job or a spaces operation runs.
 */
@Composable
private fun BackupRestoreDestination(
    route: BackupRestoreRoute,
    active: Boolean,
    factory: ViewModelProvider.Factory,
    slotsState: SlotsUiState,
    contentPadding: PaddingValues,
    onLeave: () -> Unit,
) {
    val backupViewModel: BackupRestoreViewModel = viewModel(factory = factory)
    val backupState by backupViewModel.state.collectAsStateWithLifecycle()
    LaunchedEffect(backupViewModel) {
        backupViewModel.openRoute(route.packageName, route.slotId)
    }
    val leave: () -> Unit = {
        if (active && !slotsState.busy && !backupState.running) {
            backupViewModel.onIntent(BackupUiIntent.Leave)
            onLeave()
        }
    }
    BackHandler(enabled = active, onBack = leave)
    // Lives as long as this page is composed, so the window stays secure through the page's exit
    // transition too.
    SecureWindowEffect()
    BackupRestoreScreen(
        state = backupState,
        slotsState = slotsState,
        contentPadding = contentPadding,
        onBack = leave,
        onIntent = backupViewModel::onIntent,
        suggestedBackupName = backupViewModel::suggestedBackupName,
    )
}

/** Sheets, the blocking operation alert and the top notice, drawn above the recorded pages. */
@Composable
private fun RootOverlays(
    state: SlotsUiState,
    onIntent: (UiIntent) -> Unit,
    noticeModifier: Modifier,
) {
    val configuring = rememberRetained(state.pendingConfigurationPackage)
    if (configuring != null) {
        ConfigureAppSheet(
            visible = state.pendingConfigurationPackage != null,
            packageName = configuring,
            installedApp = state.installedApps.firstOrNull { it.packageName == configuring },
            onConfirm = { onIntent(UiIntent.ConfirmConfigureApp) },
            onDismiss = { onIntent(UiIntent.DismissConfigureApp) },
        )
    }

    val recovery = rememberRetained(state.registrationRecovery)
    if (recovery != null) {
        RegistrationRecoverySheet(
            visible = state.registrationRecovery != null,
            recovery = recovery,
            onConfirmationChanged = {
                onIntent(UiIntent.RegistrationRecoveryConfirmationChanged(it))
            },
            onConfirm = { onIntent(UiIntent.ConfirmRegistrationRecovery) },
            onDismiss = { onIntent(UiIntent.DismissRegistrationRecovery) },
        )
    }

    val rename = rememberRetained(state.pendingRename)
    if (rename != null) {
        RenameSpaceSheet(
            visible = state.pendingRename != null,
            rename = rename,
            onNameChanged = { onIntent(UiIntent.RenameSlotNameChanged(it)) },
            onConfirm = { onIntent(UiIntent.ConfirmRenameSlot) },
            onDismiss = { onIntent(UiIntent.DismissRenameSlot) },
        )
    }

    val deletion = rememberRetained(state.pendingDelete)
    if (deletion != null) {
        DeleteSpaceSheet(
            visible = state.pendingDelete != null,
            deletion = deletion,
            onConfirm = { onIntent(UiIntent.ConfirmDeleteSlot) },
            onDismiss = { onIntent(UiIntent.DismissDeleteSlot) },
        )
    }

    val unenrollment = rememberRetained(state.pendingUnenroll)
    if (unenrollment != null) {
        UnenrollAppSheet(
            visible = state.pendingUnenroll != null,
            unenrollment = unenrollment,
            onConfirmationChanged = {
                onIntent(UiIntent.UnenrollConfirmationChanged(it))
            },
            onConfirm = { onIntent(UiIntent.ConfirmUnenrollApp) },
            onDismiss = { onIntent(UiIntent.DismissUnenrollApp) },
        )
    }

    val blocking = rememberRetained(state.operation.takeIf { it.blocksScreen })
    if (blocking != null) {
        OperationDialog(
            visible = state.operation.blocksScreen,
            operation = blocking,
        )
    }

    TopNoticeHost(
        posted = state.posted,
        onIntent = onIntent,
        modifier = noticeModifier,
    )
}

@Composable
private fun RenameSpaceSheet(
    visible: Boolean,
    rename: RenameSpaceUi,
    onNameChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val normalized = rename.name.trim()
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.rename_space_title),
        summary = stringResource(R.string.rename_space_hint),
    ) {
        SlicesInputField(
            value = rename.name,
            onValueChange = onNameChanged,
            label = stringResource(R.string.space_name),
            modifier = Modifier.fillMaxWidth(),
        )
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.save),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = normalized.isNotEmpty() && normalized != rename.originalName,
        )
    }
}

@Composable
private fun DeleteSpaceSheet(
    visible: Boolean,
    deletion: DeleteSpaceUi,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.delete_space_title),
        summary = stringResource(R.string.delete_space_message, deletion.name),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(
                text = stringResource(R.string.delete_space_data_warning),
                tone = Tone.Danger,
                gutter = false,
                animateText = false,
            )
            SheetNote(stringResource(R.string.delete_space_current_preserved))
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.delete_space_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            destructive = true,
        )
    }
}

@Composable
private fun UnenrollAppSheet(
    visible: Boolean,
    unenrollment: UnenrollAppUi,
    onConfirmationChanged: (Boolean) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val affectedSummary = if (unenrollment.affectedSpaces.isEmpty()) {
        stringResource(R.string.unenroll_no_spaces)
    } else {
        stringResource(
            R.string.unenroll_affected_spaces,
            joinSpaceNames(unenrollment.affectedSpaces),
        )
    }
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.unenroll_app_title),
        summary = stringResource(
            R.string.unenroll_app_message,
            unenrollment.appLabel,
            unenrollment.packageName,
        ),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(text = affectedSummary, tone = Tone.Danger, gutter = false)
            SheetNote(stringResource(R.string.unenroll_base_preserved))
            SwitchRow(
                title = stringResource(
                    if (unenrollment.affectedSpaces.isEmpty()) {
                        R.string.unenroll_confirmation_toggle_no_spaces
                    } else {
                        R.string.unenroll_confirmation_toggle
                    },
                ),
                checked = unenrollment.confirmed,
                onCheckedChange = { if (visible) onConfirmationChanged(it) },
                titleColor = Tone.Danger.onContainer(),
                horizontalPadding = Space.xxs,
            )
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.unenroll_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = unenrollment.confirmed,
            destructive = true,
        )
    }
}

@Composable
private fun ConfigureAppSheet(
    visible: Boolean,
    packageName: String,
    installedApp: InstalledApp?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val appLabel = installedApp?.label ?: packageName
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.configure_app_title),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(Radius.row))
                .background(Surfaces.control)
                .padding(Space.sm),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            AppIcon(appLabel, installedApp?.icon, 44.dp)
            Column(Modifier.weight(1f)) {
                Text(
                    text = appLabel,
                    color = TextPrimary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = packageName,
                    color = TextSecondary,
                    fontSize = TypeScale.caption,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.height(Space.sm))
        SheetNote(stringResource(R.string.configure_app_message))
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.configure_app),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
        )
    }
}

@Composable
private fun RegistrationRecoverySheet(
    visible: Boolean,
    recovery: RegistrationRecoveryUi,
    onConfirmationChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val confirmationWord = stringResource(R.string.rebind_confirmation_word)
    val affectedSummary = if (recovery.affectedSpaces.isEmpty()) {
        stringResource(R.string.rebind_no_named_spaces)
    } else {
        stringResource(
            R.string.rebind_affected_spaces,
            joinSpaceNames(recovery.affectedSpaces),
        )
    }
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.rebind_title),
        summary = stringResource(R.string.rebind_message, recovery.packageName),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(text = affectedSummary, tone = Tone.Info, gutter = false)
            SheetNote(stringResource(R.string.rebind_data_preserved))
            SlicesInputField(
                value = recovery.confirmation,
                onValueChange = { if (visible) onConfirmationChanged(it) },
                label = stringResource(R.string.rebind_confirmation_label, confirmationWord),
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                text = stringResource(R.string.rebind_confirmation_hint),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
                modifier = Modifier.padding(horizontal = Space.xxs),
            )
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.rebind_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = recovery.confirmation == confirmationWord,
        )
    }
}

/** Secondary paragraph inside a sheet. */
@Composable
private fun SheetNote(text: String) {
    Text(
        text = text,
        color = TextSecondary,
        fontSize = TypeScale.subhead,
        lineHeight = 20.sp,
        modifier = Modifier.padding(horizontal = Space.xxs),
    )
}

@Composable
private fun OperationDialog(visible: Boolean, operation: OperationUiState) {
    val message = when (operation) {
        OperationUiState.Idle -> ""
        OperationUiState.Refreshing -> stringResource(R.string.operation_refreshing)
        is OperationUiState.PreparingConfiguration ->
            stringResource(R.string.operation_preparing_configuration)
        is OperationUiState.Configuring -> stringResource(R.string.operation_configuring)
        is OperationUiState.OpeningPackage -> stringResource(R.string.operation_opening)
        is OperationUiState.CreatingSpace -> stringResource(
            R.string.operation_creating,
            operation.spaceName,
        )
        is OperationUiState.ActivatingSpace -> if (operation.switched) {
            stringResource(R.string.operation_switching, operation.spaceName)
        } else {
            stringResource(R.string.operation_launching, operation.spaceName)
        }
        is OperationUiState.RenamingSpace ->
            stringResource(R.string.operation_renaming, operation.spaceName)
        is OperationUiState.DeletingSpace ->
            stringResource(R.string.operation_deleting, operation.spaceName)
        is OperationUiState.UnenrollingApp ->
            stringResource(R.string.operation_unenrolling, operation.appLabel)
        is OperationUiState.SavingRebootLaunch ->
            stringResource(R.string.operation_saving_reboot_launch)
        is OperationUiState.RebindingConfiguration ->
            stringResource(R.string.operation_rebinding)
        OperationUiState.RestartingRuntime ->
            stringResource(R.string.operation_restarting_runtime)
    }
    CenterDialog(visible = visible) {
        Box(
            Modifier
                .size(64.dp)
                .clip(CircleShape)
                .background(Tone.Info.container()),
            contentAlignment = Alignment.Center,
        ) {
            SlicesSpinner(size = 28.dp, color = Tone.Info.onContainer(), strokeWidth = 3.dp)
        }
        Spacer(Modifier.height(Space.md))
        AnimatedContent(message, transitionSpec = fadeThrough(), label = "operation-title") {
            Text(
                text = it,
                color = TextPrimary,
                fontSize = TypeScale.headline,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(Space.xs))
        Text(
            text = stringResource(R.string.operation_wait_hint),
            color = TextSecondary,
            fontSize = TypeScale.subhead,
            lineHeight = 20.sp,
            textAlign = TextAlign.Center,
        )
    }
}
