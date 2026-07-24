package com.uclone.slices.v2.ui

import android.graphics.drawable.Drawable
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.core.graphics.drawable.toBitmap
import com.uclone.slices.v2.R

@Composable
internal fun SlicesScreen(
    state: SlotsUiState,
    onIntent: (UiIntent) -> Unit,
) {
    BackHandler(
        enabled = state.destination != ManagerDestination.Spaces && !state.busy,
    ) {
        onIntent(UiIntent.NavigateBack)
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        bottomBar = {
            state.notice?.let { notice ->
                NoticeBar(
                    notice = notice,
                    onIntent = onIntent,
                )
            }
        },
    ) { contentPadding ->
        when (state.destination) {
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
            ManagerDestination.RuntimeStatus -> RuntimeStatusScreen(
                state = state,
                contentPadding = contentPadding,
                onIntent = onIntent,
            )
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
    AlertDialog(
        onDismissRequest = onDismiss,
        shape = RoundedCornerShape(20.dp),
        title = { Text(stringResource(R.string.rename_space_title)) },
        text = {
            OutlinedTextField(
                value = rename.name,
                onValueChange = onNameChanged,
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                label = { Text(stringResource(R.string.space_name)) },
                supportingText = { Text(stringResource(R.string.rename_space_hint)) },
                shape = RoundedCornerShape(12.dp),
            )
        },
        confirmButton = {
            Button(
                onClick = onConfirm,
                enabled = normalized.isNotEmpty() && normalized != rename.originalName,
            ) {
                Text(stringResource(R.string.save))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

@Composable
private fun DeleteSpaceDialog(
    deletion: DeleteSpaceUi,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        shape = RoundedCornerShape(20.dp),
        title = { Text(stringResource(R.string.delete_space_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = stringResource(R.string.delete_space_message, deletion.name),
                )
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.errorContainer,
                ) {
                    Text(
                        text = stringResource(R.string.delete_space_data_warning),
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(12.dp),
                    )
                }
                Text(
                    text = stringResource(R.string.delete_space_current_preserved),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = {
            Button(
                onClick = onConfirm,
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.error,
                    contentColor = MaterialTheme.colorScheme.onError,
                ),
            ) {
                Text(stringResource(R.string.delete_space_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
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
    AlertDialog(
        onDismissRequest = onDismiss,
        shape = RoundedCornerShape(20.dp),
        title = { Text(stringResource(R.string.unenroll_app_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = stringResource(
                        R.string.unenroll_app_message,
                        unenrollment.appLabel,
                        unenrollment.packageName,
                    ),
                )
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.errorContainer,
                ) {
                    Text(
                        text = affectedSummary,
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(12.dp),
                    )
                }
                Text(
                    text = stringResource(R.string.unenroll_base_preserved),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    value = unenrollment.confirmation,
                    onValueChange = onConfirmationChanged,
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    label = { Text(stringResource(R.string.unenroll_confirmation_label)) },
                    supportingText = {
                        Text(stringResource(R.string.unenroll_confirmation_hint))
                    },
                    shape = RoundedCornerShape(12.dp),
                )
            }
        },
        confirmButton = {
            Button(
                onClick = onConfirm,
                enabled = unenrollment.confirmation == RECOVERY_CONFIRMATION,
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.error,
                    contentColor = MaterialTheme.colorScheme.onError,
                ),
            ) {
                Text(stringResource(R.string.unenroll_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

@Composable
private fun ConfigureAppDialog(
    appLabel: String,
    packageName: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        shape = RoundedCornerShape(20.dp),
        title = { Text(stringResource(R.string.configure_app_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    text = appLabel,
                    style = MaterialTheme.typography.titleMedium,
                )
                Text(
                    text = packageName,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    text = stringResource(R.string.configure_app_message),
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        },
        confirmButton = {
            Button(onClick = onConfirm) {
                Text(stringResource(R.string.configure_app))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

@Composable
private fun RegistrationRecoveryDialog(
    recovery: RegistrationRecoveryUi,
    onConfirmationChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val affectedSummary = if (recovery.affectedSpaces.isEmpty()) {
        stringResource(R.string.reset_no_named_spaces)
    } else {
        stringResource(
            R.string.reset_affected_spaces,
            recovery.affectedSpaces.joinToString("、"),
        )
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        shape = RoundedCornerShape(20.dp),
        title = { Text(stringResource(R.string.reset_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = stringResource(
                        R.string.reset_message,
                        recovery.packageName,
                    ),
                )
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.errorContainer,
                ) {
                    Text(
                        text = affectedSummary,
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(12.dp),
                    )
                }
                Text(
                    text = stringResource(R.string.reset_base_preserved),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    value = recovery.confirmation,
                    onValueChange = onConfirmationChanged,
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    label = { Text(stringResource(R.string.reset_confirmation_label)) },
                    supportingText = {
                        Text(stringResource(R.string.reset_confirmation_hint))
                    },
                    shape = RoundedCornerShape(12.dp),
                )
            }
        },
        confirmButton = {
            Button(
                onClick = onConfirm,
                enabled = recovery.confirmation == RECOVERY_CONFIRMATION,
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.error,
                    contentColor = MaterialTheme.colorScheme.onError,
                ),
            ) {
                Text(stringResource(R.string.reset_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
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
        is OperationUiState.RepairingConfiguration ->
            stringResource(R.string.operation_repairing)
    }
    Dialog(onDismissRequest = {}) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = MaterialTheme.colorScheme.surface,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 22.dp),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(
                    modifier = Modifier.size(24.dp),
                    strokeWidth = 2.5.dp,
                )
                Column {
                    Text(
                        text = message,
                        style = MaterialTheme.typography.titleMedium,
                    )
                    Text(
                        text = stringResource(R.string.operation_wait_hint),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
internal fun RuntimeStatusCard(
    runtimeReady: Boolean,
    busy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accentColor = when {
        busy -> MaterialTheme.colorScheme.primary
        runtimeReady -> SlicesSuccess
        else -> SlicesWarning
    }
    val title = when {
        busy -> stringResource(R.string.environment_processing)
        runtimeReady -> stringResource(R.string.environment_ready)
        else -> stringResource(R.string.environment_unavailable)
    }
    val description = if (runtimeReady) {
        stringResource(R.string.environment_ready_description)
    } else {
        stringResource(R.string.environment_unavailable_description)
    }

    Card(
        modifier = modifier.fillMaxWidth(),
        enabled = enabled,
        onClick = onClick,
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface,
            disabledContainerColor = MaterialTheme.colorScheme.surface,
        ),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
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
        notice is UiNotice.ConfigurationRepaired ||
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
            TextButton(
                onClick = {
                    onIntent(if (retry) UiIntent.Refresh else UiIntent.DismissNotice)
                },
            ) {
                Text(actionLabel)
            }
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
    UiNotice.ConfigurationCompleted -> stringResource(R.string.success_configured)
    UiNotice.ConfigurationRepaired -> stringResource(R.string.success_repaired)
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
