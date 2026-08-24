package com.uclone.slices.v2.ui

import android.graphics.drawable.Drawable
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.core.graphics.drawable.toBitmap
import com.uclone.slices.v2.R
import com.uclone.slices.v2.backup.BackupRestoreScreen
import com.uclone.slices.v2.backup.BackupRestoreUiState
import com.uclone.slices.v2.backup.BackupUiIntent
import top.yukonga.miuix.kmp.basic.CircularProgressIndicator as MiuixCircularProgressIndicator
import top.yukonga.miuix.kmp.basic.Scaffold
import top.yukonga.miuix.kmp.extra.SuperDialog
import top.yukonga.miuix.kmp.theme.MiuixTheme

@Composable
internal fun SlicesScreen(
    state: SlotsUiState,
    backupState: BackupRestoreUiState,
    onIntent: (UiIntent) -> Unit,
    onBackupIntent: (BackupUiIntent) -> Unit,
    suggestedBackupName: () -> String,
) {
    BackHandler(
        enabled = state.destination != ManagerDestination.Spaces &&
            !state.busy &&
            !backupState.running,
    ) {
        onIntent(UiIntent.NavigateBack)
    }

    Scaffold(
        containerColor = MiuixTheme.colorScheme.surface,
        bottomBar = {
            state.notice?.let { notice ->
                NoticeBar(
                    notice = notice,
                    onIntent = onIntent,
                )
            }
        },
    ) { contentPadding ->
        AnimatedContent(
            targetState = state.destination,
            transitionSpec = {
                val forward = targetState != ManagerDestination.Spaces
                val enterOffset: (Int) -> Int = { width ->
                    if (forward) width / 5 else -width / 5
                }
                val exitOffset: (Int) -> Int = { width ->
                    if (forward) -width / 8 else width / 8
                }
                (
                    slideInHorizontally(tween(220), enterOffset) +
                        fadeIn(tween(180))
                    ) togetherWith (
                    slideOutHorizontally(tween(180), exitOffset) +
                        fadeOut(tween(140))
                    )
            },
            label = "manager_destination",
            modifier = Modifier.fillMaxSize(),
        ) { destination ->
            when (destination) {
                ManagerDestination.Spaces -> AppListScreen(
                    state = state,
                    contentPadding = contentPadding,
                    onIntent = onIntent,
                )
                ManagerDestination.AddApp -> AddAppScreen(
                    state = state,
                    contentPadding = contentPadding,
                    onIntent = onIntent,
                )
                ManagerDestination.PackageDetails -> state.selected?.let { selected ->
                    PackageDetailScreen(
                        state = state,
                        packageSnapshot = selected,
                        installedApp = state.installedApps.firstOrNull {
                            it.packageName == selected.packageName
                        },
                        contentPadding = contentPadding,
                        onIntent = onIntent,
                    )
                }
                ManagerDestination.BackupRestore -> BackupRestoreScreen(
                    state = backupState,
                    slotsState = state,
                    contentPadding = contentPadding,
                    onBack = { onIntent(UiIntent.NavigateBack) },
                    onIntent = onBackupIntent,
                    suggestedBackupName = suggestedBackupName,
                )
                ManagerDestination.RuntimeStatus -> RuntimeStatusScreen(
                    state = state,
                    contentPadding = contentPadding,
                    onIntent = onIntent,
                )
            }
        }
    }

    state.pendingConfigurationPackage?.let { packageName ->
        val installedApp = state.installedApps.firstOrNull {
            it.packageName == packageName
        }
        ConfigureAppDialog(
            appLabel = installedApp?.label ?: packageName,
            packageName = packageName,
            onConfirm = { onIntent(UiIntent.ConfirmConfigureApp) },
            onDismiss = { onIntent(UiIntent.DismissConfigureApp) },
        )
    }

    state.registrationRecovery?.let { recovery ->
        RegistrationRecoveryDialog(
            recovery = recovery,
            onConfirmationChanged = {
                onIntent(UiIntent.RegistrationRecoveryConfirmationChanged(it))
            },
            onConfirm = { onIntent(UiIntent.ConfirmRegistrationRecovery) },
            onDismiss = { onIntent(UiIntent.DismissRegistrationRecovery) },
        )
    }

    state.pendingRename?.let { rename ->
        RenameSpaceDialog(
            rename = rename,
            onNameChanged = { onIntent(UiIntent.RenameSlotNameChanged(it)) },
            onConfirm = { onIntent(UiIntent.ConfirmRenameSlot) },
            onDismiss = { onIntent(UiIntent.DismissRenameSlot) },
        )
    }

    state.pendingDelete?.let { deletion ->
        DeleteSpaceDialog(
            deletion = deletion,
            onConfirm = { onIntent(UiIntent.ConfirmDeleteSlot) },
            onDismiss = { onIntent(UiIntent.DismissDeleteSlot) },
        )
    }

    state.pendingUnenroll?.let { unenrollment ->
        UnenrollAppDialog(
            unenrollment = unenrollment,
            onConfirmationChanged = {
                onIntent(UiIntent.UnenrollConfirmationChanged(it))
            },
            onConfirm = { onIntent(UiIntent.ConfirmUnenrollApp) },
            onDismiss = { onIntent(UiIntent.DismissUnenrollApp) },
        )
    }

    if (state.busy) {
        OperationDialog(state.operation)
    }
}

@Composable
private fun RenameSpaceDialog(
    rename: RenameSpaceUi,
    onNameChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val normalized = rename.name.trim()
    val show = remember(rename.packageName, rename.slotId) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = stringResource(R.string.rename_space_title),
        summary = stringResource(R.string.rename_space_hint),
        onDismissRequest = {
            show.value = false
            onDismiss()
        },
    ) {
        SlicesInputField(
            value = rename.name,
            onValueChange = onNameChanged,
            label = stringResource(R.string.space_name),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SlicesTextAction(
                text = stringResource(R.string.cancel),
                onClick = {
                    show.value = false
                    onDismiss()
                },
                modifier = Modifier.weight(1f),
            )
            SlicesTextAction(
                text = stringResource(R.string.save),
                onClick = {
                    show.value = false
                    onConfirm()
                },
                enabled = normalized.isNotEmpty() && normalized != rename.originalName,
                primary = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun DeleteSpaceDialog(
    deletion: DeleteSpaceUi,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val show = remember(deletion.packageName, deletion.slotId) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = stringResource(R.string.delete_space_title),
        onDismissRequest = {
            show.value = false
            onDismiss()
        },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                text = stringResource(R.string.delete_space_message, deletion.name),
            )
            SlicesPanel(
                insideMargin = androidx.compose.foundation.layout.PaddingValues(12.dp),
                cornerRadius = 14.dp,
                color = MaterialTheme.colorScheme.errorContainer,
            ) {
                Text(
                    text = stringResource(R.string.delete_space_data_warning),
                    color = MaterialTheme.colorScheme.onErrorContainer,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            Text(
                text = stringResource(R.string.delete_space_current_preserved),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 18.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SlicesTextAction(
                text = stringResource(R.string.cancel),
                onClick = {
                    show.value = false
                    onDismiss()
                },
                modifier = Modifier.weight(1f),
            )
            SlicesTextAction(
                text = stringResource(R.string.delete_space_confirm),
                onClick = {
                    show.value = false
                    onConfirm()
                },
                danger = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun UnenrollAppDialog(
    unenrollment: UnenrollAppUi,
    onConfirmationChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val affectedSummary = if (unenrollment.affectedSpaces.isEmpty()) {
        stringResource(R.string.unenroll_no_spaces)
    } else {
        stringResource(
            R.string.unenroll_affected_spaces,
            unenrollment.affectedSpaces.joinToString("、"),
        )
    }
    val show = remember(unenrollment.packageName) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = stringResource(R.string.unenroll_app_title),
        summary = stringResource(
            R.string.unenroll_app_message,
            unenrollment.appLabel,
            unenrollment.packageName,
        ),
        insideMargin = DpSize(22.dp, 16.dp),
        onDismissRequest = {
            show.value = false
            onDismiss()
        },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SlicesPanel(
                insideMargin = androidx.compose.foundation.layout.PaddingValues(10.dp),
                cornerRadius = 12.dp,
                color = MaterialTheme.colorScheme.errorContainer,
            ) {
                Text(
                    text = affectedSummary,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                text = stringResource(R.string.unenroll_base_preserved),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            SlicesInputField(
                value = unenrollment.confirmation,
                onValueChange = onConfirmationChanged,
                label = stringResource(R.string.unenroll_confirmation_label),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SlicesTextAction(
                text = stringResource(R.string.cancel),
                onClick = {
                    show.value = false
                    onDismiss()
                },
                modifier = Modifier.weight(1f),
            )
            SlicesTextAction(
                text = stringResource(R.string.unenroll_confirm),
                onClick = {
                    show.value = false
                    onConfirm()
                },
                enabled = unenrollment.confirmation == DELETION_CONFIRMATION,
                danger = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun ConfigureAppDialog(
    appLabel: String,
    packageName: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val show = remember(packageName) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = stringResource(R.string.configure_app_title),
        summary = packageName,
        onDismissRequest = {
            show.value = false
            onDismiss()
        },
    ) {
        Text(
            text = appLabel,
            style = MaterialTheme.typography.titleMedium,
        )
        Text(
            text = stringResource(R.string.configure_app_message),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 8.dp),
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 18.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SlicesTextAction(
                text = stringResource(R.string.cancel),
                onClick = {
                    show.value = false
                    onDismiss()
                },
                modifier = Modifier.weight(1f),
            )
            SlicesTextAction(
                text = stringResource(R.string.configure_app),
                onClick = {
                    show.value = false
                    onConfirm()
                },
                primary = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun RegistrationRecoveryDialog(
    recovery: RegistrationRecoveryUi,
    onConfirmationChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val affectedSummary = if (recovery.affectedSpaces.isEmpty()) {
        stringResource(R.string.rebind_no_named_spaces)
    } else {
        stringResource(
            R.string.rebind_affected_spaces,
            recovery.affectedSpaces.joinToString("、"),
        )
    }
    val show = remember(recovery.packageName) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = stringResource(R.string.rebind_title),
        onDismissRequest = {
            show.value = false
            onDismiss()
        },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                text = stringResource(
                    R.string.rebind_message,
                    recovery.packageName,
                ),
            )
            SlicesPanel(
                insideMargin = androidx.compose.foundation.layout.PaddingValues(12.dp),
                cornerRadius = 14.dp,
                color = MaterialTheme.colorScheme.primaryContainer,
            ) {
                Text(
                    text = affectedSummary,
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            Text(
                text = stringResource(R.string.rebind_data_preserved),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            SlicesInputField(
                value = recovery.confirmation,
                onValueChange = onConfirmationChanged,
                label = stringResource(R.string.rebind_confirmation_label),
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                text = stringResource(R.string.rebind_confirmation_hint),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 18.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SlicesTextAction(
                text = stringResource(R.string.cancel),
                onClick = {
                    show.value = false
                    onDismiss()
                },
                modifier = Modifier.weight(1f),
            )
            SlicesTextAction(
                text = stringResource(R.string.rebind_confirm),
                onClick = {
                    show.value = false
                    onConfirm()
                },
                enabled = recovery.confirmation == BINDING_CONFIRMATION,
                primary = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun OperationDialog(operation: OperationUiState) {
    val message = when (operation) {
        OperationUiState.Idle -> return
        OperationUiState.Refreshing -> stringResource(R.string.operation_refreshing)
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
    }
    val show = remember(operation) { mutableStateOf(true) }
    SuperDialog(
        show = show,
        title = message,
        summary = stringResource(R.string.operation_wait_hint),
        enableWindowDim = true,
        onDismissRequest = null,
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 6.dp),
            contentAlignment = Alignment.Center,
        ) {
            MiuixCircularProgressIndicator(modifier = Modifier.size(28.dp))
        }
    }
}

@Composable
internal fun RuntimeStatusCard(
    runtimeReady: Boolean,
    runtimeCompatible: Boolean,
    busy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accentColor = when {
        busy -> MaterialTheme.colorScheme.primary
        runtimeReady && runtimeCompatible -> SlicesSuccess
        else -> SlicesWarning
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

    SlicesPanel(
        modifier = modifier.fillMaxWidth(),
        enabled = enabled,
        onClick = onClick,
        cornerRadius = 18.dp,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 15.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Surface(
                modifier = Modifier.size(10.dp),
                shape = RoundedCornerShape(5.dp),
                color = accentColor,
                content = {},
            )
            Column(
                modifier = Modifier
                    .weight(1f)
                    .padding(horizontal = 12.dp),
            ) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                )
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun AppIcon(
    label: String,
    icon: Drawable?,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val bitmap = remember(icon) {
        icon?.toBitmap(width = 128, height = 128)?.asImageBitmap()
    }
    if (bitmap != null) {
        Image(
            bitmap = bitmap,
            contentDescription = null,
            modifier = modifier
                .size(size)
                .clip(RoundedCornerShape(size / 4)),
        )
    } else {
        Surface(
            modifier = modifier.size(size),
            shape = RoundedCornerShape(size / 4),
            color = MaterialTheme.colorScheme.primaryContainer,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(
                    text = label.trim().take(1).ifEmpty { "?" },
                    style = MaterialTheme.typography.titleLarge,
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                )
            }
        }
    }
}

@Composable
private fun NoticeBar(
    notice: UiNotice,
    onIntent: (UiIntent) -> Unit,
) {
    val success = notice is UiNotice.ConfigurationCompleted ||
        notice is UiNotice.ConfigurationRebound ||
        notice is UiNotice.SpaceCreated ||
        notice is UiNotice.SpaceActivated ||
        notice is UiNotice.SpaceRenamed ||
        notice is UiNotice.SpaceDeleted ||
        notice is UiNotice.AppUnenrolled
    val message = noticeMessage(notice)
    val retry = notice is UiNotice.LocalAppsUnavailable ||
        notice is UiNotice.RuntimeUnavailable ||
        notice is UiNotice.MissingEntity ||
        notice is UiNotice.StateChanged ||
        notice is UiNotice.OperationFailed
    val actionLabel = if (retry) {
        stringResource(R.string.retry)
    } else {
        stringResource(R.string.dismiss)
    }

    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .navigationBarsPadding(),
        color = if (success) {
            SlicesSuccessContainer
        } else {
            MaterialTheme.colorScheme.errorContainer
        },
    ) {
        Row(
            modifier = Modifier.padding(start = 20.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = message,
                color = if (success) SlicesSuccess else MaterialTheme.colorScheme.onErrorContainer,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.weight(1f),
            )
            SlicesInlineAction(
                text = actionLabel,
                onClick = {
                    onIntent(if (retry) UiIntent.Refresh else UiIntent.DismissNotice)
                },
                color = if (success) {
                    SlicesSuccess
                } else {
                    MaterialTheme.colorScheme.onErrorContainer
                },
            )
        }
    }
}

@Composable
private fun noticeMessage(notice: UiNotice): String = when (notice) {
    UiNotice.LocalAppsUnavailable -> stringResource(R.string.error_local_apps)
    UiNotice.RuntimeUnavailable -> stringResource(R.string.error_runtime_unavailable)
    UiNotice.InvalidInput -> stringResource(R.string.error_invalid_input)
    UiNotice.MissingEntity -> stringResource(R.string.error_missing_entity)
    UiNotice.StateChanged -> stringResource(R.string.error_state_changed)
    UiNotice.OperationFailed -> stringResource(R.string.error_operation_failed)
    UiNotice.IdentityProtected -> stringResource(R.string.error_identity_protected)
    UiNotice.SigningUnavailable -> stringResource(R.string.error_signing_unavailable)
    UiNotice.RuntimeVersionMismatch -> stringResource(R.string.error_runtime_version_mismatch)
    UiNotice.ConfigurationCompleted -> stringResource(R.string.success_configured)
    UiNotice.ConfigurationRebound -> stringResource(R.string.success_rebound)
    is UiNotice.SpaceCreated -> stringResource(R.string.success_created, notice.name)
    is UiNotice.SpaceActivated -> if (notice.switched) {
        stringResource(R.string.success_switched, notice.name)
    } else {
        stringResource(R.string.success_launched, notice.name)
    }
    is UiNotice.SpaceRenamed -> stringResource(R.string.success_renamed, notice.name)
    is UiNotice.SpaceDeleted -> stringResource(R.string.success_deleted, notice.name)
    is UiNotice.AppUnenrolled ->
        stringResource(R.string.success_unenrolled, notice.appLabel)
}
