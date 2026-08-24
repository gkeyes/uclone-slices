package com.uclone.slices.v2.backup

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.AccountIoKind
import com.uclone.slices.v2.runtime.ArchiveScope
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.RestoreItemState
import com.uclone.slices.v2.runtime.RestoreTarget
import com.uclone.slices.v2.ui.BASE_SLOT_ID
import com.uclone.slices.v2.ui.SlicesActionButton
import com.uclone.slices.v2.ui.SlicesInputField
import com.uclone.slices.v2.ui.SlicesPanel
import com.uclone.slices.v2.ui.SlicesTextAction
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.spaceDisplayName
import java.text.DateFormat
import java.util.Date

@Composable
internal fun BackupRestoreScreen(
    state: BackupRestoreUiState,
    slotsState: SlotsUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
    suggestedBackupName: () -> String,
) {
    val createDocument = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri -> uri?.let { onIntent(BackupUiIntent.StartBackup(it)) } }
    val openDocument = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri -> uri?.let { onIntent(BackupUiIntent.SelectRestore(it)) } }

    if (state.running) {
        RunningContent(state, contentPadding)
        return
    }

    when (state.page) {
        BackupRestorePage.Landing -> LandingContent(
            state = state,
            slotsState = slotsState,
            contentPadding = contentPadding,
            onBack = onBack,
            onIntent = onIntent,
            onChooseBackup = { onIntent(BackupUiIntent.ChooseBackupPackage(it)) },
            onChooseRestore = { openDocument.launch(arrayOf("*/*")) },
        )
        BackupRestorePage.BackupSetup -> BackupSetupContent(
            state = state,
            slotsState = slotsState,
            contentPadding = contentPadding,
            onBack = onBack,
            onIntent = onIntent,
            onStart = { createDocument.launch(suggestedBackupName()) },
        )
        BackupRestorePage.RestorePassword -> RestorePasswordContent(
            state = state,
            contentPadding = contentPadding,
            onBack = onBack,
            onIntent = onIntent,
        )
        BackupRestorePage.RestorePreview -> RestorePreviewContent(
            state = state,
            contentPadding = contentPadding,
            onBack = onBack,
            onIntent = onIntent,
        )
        BackupRestorePage.Result -> ResultContent(
            state = state,
            contentPadding = contentPadding,
            onBack = onBack,
            onIntent = onIntent,
            onDismiss = { onIntent(BackupUiIntent.DismissResult) },
        )
    }
}

@Composable
private fun BackupHeader(title: String, onBack: () -> Unit, enabled: Boolean = true) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SlicesTextAction(
            text = stringResource(R.string.back),
            onClick = onBack,
            enabled = enabled,
        )
        Column(Modifier.padding(start = 8.dp)) {
            Text(text = title, style = MaterialTheme.typography.headlineSmall)
            Text(
                text = stringResource(R.string.backup_restore_subtitle),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun LandingContent(
    state: BackupRestoreUiState,
    slotsState: SlotsUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
    onChooseBackup: (String) -> Unit,
    onChooseRestore: () -> Unit,
) {
    val eligiblePackages = slotsState.packages.filter { it.bindingState == BindingState.Ready }
    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(contentPadding),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item { BackupHeader(stringResource(R.string.backup_restore_title), onBack) }
        if (state.accountIoStatuses.isNotEmpty() || state.accountIoRecoveryError != null) {
            item { AccountIoRecoveryContent(state, onIntent) }
        }
        item {
            SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp)) {
                    Text(
                        text = stringResource(R.string.restore_from_file),
                        style = MaterialTheme.typography.titleMedium,
                    )
                    Text(
                        text = stringResource(R.string.restore_from_file_description),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                    SlicesActionButton(
                        text = stringResource(R.string.choose_backup_file),
                        onClick = onChooseRestore,
                        enabled = slotsState.operationsAllowed,
                        icon = painterResource(R.drawable.ic_restore),
                        modifier = Modifier.fillMaxWidth().padding(top = 14.dp),
                    )
                }
            }
        }
        item {
            Text(
                text = stringResource(R.string.create_account_backup),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
        if (eligiblePackages.isEmpty()) {
            item {
                Text(
                    text = stringResource(R.string.no_backup_apps),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        } else {
            items(eligiblePackages, key = { it.packageName }) { snapshot ->
                val label = slotsState.installedApps.firstOrNull {
                    it.packageName == snapshot.packageName
                }?.label ?: snapshot.packageName
                SlicesPanel(
                    modifier = Modifier.fillMaxWidth(),
                    enabled = slotsState.operationsAllowed,
                    onClick = { onChooseBackup(snapshot.packageName) },
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ic_backup),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(28.dp),
                        )
                        Column(Modifier.weight(1f).padding(start = 12.dp)) {
                            Text(label, style = MaterialTheme.typography.titleMedium)
                            Text(
                                stringResource(
                                    R.string.backup_account_count,
                                    snapshot.slots.size,
                                ),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
        state.errorCode?.let { code -> item { BackupErrorText(code) } }
    }
}

@Composable
private fun BackupSetupContent(
    state: BackupRestoreUiState,
    slotsState: SlotsUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
    onStart: () -> Unit,
) {
    val snapshot = slotsState.packages.firstOrNull { it.packageName == state.packageName }
    val app = slotsState.installedApps.firstOrNull { it.packageName == state.packageName }
    val selectedSlot = snapshot?.slots?.firstOrNull { it.id == state.slotId }
    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(contentPadding),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item { BackupHeader(stringResource(R.string.create_account_backup), onBack) }
        item {
            SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp)) {
                    Text(
                        text = app?.label ?: state.packageName.orEmpty(),
                        style = MaterialTheme.typography.titleLarge,
                    )
                    Text(
                        text = if (selectedSlot == null) {
                            stringResource(R.string.backup_all_accounts)
                        } else {
                            stringResource(
                                R.string.backup_one_account,
                                spaceDisplayName(selectedSlot),
                            )
                        },
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                    if (selectedSlot == null) {
                        Text(
                            text = snapshot?.slots?.joinToString("、") { spaceDisplayName(it) }
                                .orEmpty(),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                    }
                }
            }
        }
        item {
            SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                stringResource(R.string.password_protection),
                                style = MaterialTheme.typography.titleMedium,
                            )
                            Text(
                                stringResource(R.string.password_protection_default),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Switch(
                            checked = state.passwordProtected,
                            onCheckedChange = {
                                onIntent(BackupUiIntent.PasswordProtectionChanged(it))
                            },
                        )
                    }
                    if (state.passwordProtected) {
                        PasswordField(
                            value = state.password,
                            label = stringResource(R.string.backup_password),
                            onValueChange = { onIntent(BackupUiIntent.PasswordChanged(it)) },
                            modifier = Modifier.fillMaxWidth().padding(top = 14.dp),
                        )
                        PasswordField(
                            value = state.passwordConfirmation,
                            label = stringResource(R.string.confirm_backup_password),
                            onValueChange = {
                                onIntent(BackupUiIntent.PasswordConfirmationChanged(it))
                            },
                            modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
                        )
                        Text(
                            text = stringResource(R.string.password_loss_warning),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(top = 10.dp),
                        )
                    } else {
                        Text(
                            text = stringResource(R.string.unencrypted_backup_warning),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(top = 12.dp),
                        )
                    }
                }
            }
        }
        item {
            Text(
                text = stringResource(R.string.backup_scope_exclusions),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        state.errorCode?.let { code -> item { BackupErrorText(code) } }
        item {
            SlicesActionButton(
                text = stringResource(R.string.choose_save_location),
                onClick = onStart,
                enabled = snapshot != null &&
                    (!state.passwordProtected || (
                        state.password.isNotEmpty() &&
                            state.password == state.passwordConfirmation
                        )),
                icon = painterResource(R.drawable.ic_backup),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun RestorePasswordContent(
    state: BackupRestoreUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(contentPadding),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item { BackupHeader(stringResource(R.string.enter_backup_password), onBack) }
        item {
            SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp)) {
                    Text(
                        stringResource(R.string.encrypted_backup_description),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    PasswordField(
                        value = state.password,
                        label = stringResource(R.string.backup_password),
                        onValueChange = { onIntent(BackupUiIntent.PasswordChanged(it)) },
                        modifier = Modifier.fillMaxWidth().padding(top = 14.dp),
                    )
                }
            }
        }
        state.errorCode?.let { code -> item { BackupErrorText(code) } }
        item {
            SlicesActionButton(
                text = stringResource(R.string.decrypt_and_preview),
                onClick = { onIntent(BackupUiIntent.RetryRestorePassword) },
                enabled = state.password.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun RestorePreviewContent(
    state: BackupRestoreUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
) {
    val manifest = state.manifest ?: return
    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(contentPadding),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item { BackupHeader(stringResource(R.string.restore_preview), onBack) }
        item {
            SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(
                        state.targetApp?.label ?: manifest.packageName,
                        style = MaterialTheme.typography.titleLarge,
                    )
                    Text(manifest.packageName, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(
                        stringResource(
                            R.string.archive_summary,
                            manifest.accounts.size,
                            formatBytes(manifest.logicalSize),
                        ),
                        color = MaterialTheme.colorScheme.primary,
                    )
                    Text(
                        stringResource(
                            R.string.archive_created,
                            DateFormat.getDateTimeInstance().format(Date(manifest.createdAtMillis)),
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        stringResource(
                            R.string.archive_origin,
                            manifest.appVersion,
                            manifest.androidVersion,
                            manifest.device,
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        stringResource(
                            R.string.archive_target_version,
                            state.targetApp?.versionName.orEmpty(),
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = if (
                            state.targetApp?.versionName.orEmpty() == manifest.appVersion
                        ) {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        } else {
                            MaterialTheme.colorScheme.error
                        },
                    )
                }
            }
        }
        if (!state.signatureMatches) {
            item {
                SlicesPanel(
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.errorContainer,
                ) {
                    Text(
                        text = stringResource(R.string.signature_mismatch_warning),
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
        }
        item {
            Text(
                stringResource(R.string.restore_account_mapping),
                style = MaterialTheme.typography.titleMedium,
            )
        }
        items(state.mappings, key = { it.archiveAccountId }) { mapping ->
            MappingCard(mapping, state, onIntent)
        }
        item {
            SlicesPanel(
                modifier = Modifier.fillMaxWidth(),
                color = MaterialTheme.colorScheme.errorContainer,
            ) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        text = stringResource(R.string.restore_overwrite_warning),
                        color = MaterialTheme.colorScheme.onErrorContainer,
                    )
                    SlicesInputField(
                        value = state.destructiveConfirmation,
                        onValueChange = {
                            onIntent(BackupUiIntent.DestructiveConfirmationChanged(it))
                        },
                        label = stringResource(
                            R.string.restore_confirmation_label,
                            state.expectedConfirmation,
                        ),
                        modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
                    )
                }
            }
        }
        state.errorCode?.let { code -> item { BackupErrorText(code) } }
        item {
            SlicesActionButton(
                text = stringResource(R.string.start_restore),
                onClick = { onIntent(BackupUiIntent.StartRestore) },
                enabled = state.destructiveConfirmation == state.expectedConfirmation,
                danger = true,
                icon = painterResource(R.drawable.ic_restore),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun MappingCard(
    mapping: RestoreMappingUi,
    state: BackupRestoreUiState,
    onIntent: (BackupUiIntent) -> Unit,
) {
    var expanded by remember(mapping.archiveAccountId) { mutableStateOf(false) }
    val targetText = when (val target = mapping.target) {
        RestoreTarget.New -> stringResource(R.string.restore_as_new_account)
        is RestoreTarget.Existing -> if (target.slotId == BASE_SLOT_ID) {
            stringResource(R.string.system_original_space)
        } else {
            state.targetPackage?.slots?.firstOrNull { it.id == target.slotId }
                ?.let(::spaceDisplayName)
                ?: target.slotId
        }
    }
    SlicesPanel(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(mapping.name, style = MaterialTheme.typography.titleMedium)
            Text(
                text = if (mapping.archiveKind == ArchiveAccountKind.Base) {
                    stringResource(R.string.base_restore_fixed)
                } else {
                    stringResource(R.string.restore_target, targetText)
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 3.dp),
            )
            if (mapping.archiveKind == ArchiveAccountKind.Slot) {
                SlicesTextAction(
                    text = stringResource(R.string.change_restore_target),
                    onClick = { expanded = true },
                    primary = true,
                    modifier = Modifier.padding(top = 6.dp),
                )
                DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                    DropdownMenuItem(
                        text = { Text(stringResource(R.string.restore_as_new_account)) },
                        onClick = {
                            expanded = false
                            onIntent(
                                BackupUiIntent.RestoreMappingChanged(
                                    mapping.archiveAccountId,
                                    RestoreTarget.New,
                                ),
                            )
                        },
                    )
                    state.targetPackage?.slots?.filterNot { it.id == BASE_SLOT_ID }?.forEach { slot ->
                        DropdownMenuItem(
                            text = { Text(spaceDisplayName(slot)) },
                            onClick = {
                                expanded = false
                                onIntent(
                                    BackupUiIntent.RestoreMappingChanged(
                                        mapping.archiveAccountId,
                                        RestoreTarget.Existing(slot.id),
                                    ),
                                )
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun RunningContent(state: BackupRestoreUiState, contentPadding: PaddingValues) {
    val running = state.jobState as? BackupJobState.Running
    Column(
        modifier = Modifier.fillMaxSize().padding(contentPadding).padding(28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        CircularProgressIndicator(modifier = Modifier.size(42.dp))
        Text(
            text = when (running?.kind) {
                BackupJobKind.Backup -> stringResource(R.string.backup_running)
                BackupJobKind.Inspect -> stringResource(R.string.inspect_running)
                BackupJobKind.Restore -> stringResource(R.string.restore_running)
                null -> stringResource(R.string.backup_restore_notification_text)
            },
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(top = 18.dp),
        )
        if (running != null && running.total > 0) {
            Text(
                stringResource(R.string.restore_progress, running.completed, running.total),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 6.dp),
            )
        }
        Text(
            stringResource(R.string.do_not_force_stop),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 10.dp),
        )
    }
}

@Composable
private fun ResultContent(
    state: BackupRestoreUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
    onDismiss: () -> Unit,
) {
    val restoreResult = state.jobState as? BackupJobState.RestoreComplete
    val title: String
    val summary: String
    val success: Boolean
    when (val result = state.jobState) {
        is BackupJobState.BackupComplete -> {
            title = stringResource(R.string.backup_complete)
            summary = stringResource(
                R.string.backup_complete_summary,
                result.manifest.accounts.size,
                formatBytes(result.manifest.logicalSize),
            )
            success = true
        }
        is BackupJobState.RestoreComplete -> {
            val restored = result.result.items.count { it.state == RestoreItemState.Restored }
            val failed = result.result.items.count { it.state == RestoreItemState.Failed }
            val skipped = result.result.items.count { it.state == RestoreItemState.Skipped }
            title = stringResource(R.string.restore_complete)
            summary = stringResource(
                R.string.restore_complete_summary,
                restored,
                failed,
                skipped,
            )
            success = failed == 0 && skipped == 0
        }
        is BackupJobState.Failure -> {
            title = stringResource(R.string.backup_restore_failed)
            summary = backupErrorMessage(result.code)
            success = false
        }
        else -> {
            title = stringResource(R.string.backup_restore_failed)
            summary = backupErrorMessage("operation_failed")
            success = false
        }
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(contentPadding),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item { BackupHeader(title, onBack) }
        if (state.accountIoStatuses.isNotEmpty() || state.accountIoRecoveryError != null) {
            item { AccountIoRecoveryContent(state, onIntent) }
        }
        item {
            SlicesPanel(
                modifier = Modifier.fillMaxWidth(),
                color = if (success) {
                    MaterialTheme.colorScheme.primaryContainer
                } else {
                    MaterialTheme.colorScheme.errorContainer
                },
            ) {
                Text(
                    text = summary,
                    color = if (success) {
                        MaterialTheme.colorScheme.onPrimaryContainer
                    } else {
                        MaterialTheme.colorScheme.onErrorContainer
                    },
                    modifier = Modifier.padding(18.dp),
                )
            }
        }
        if (
            restoreResult != null &&
            !restoreResult.result.appStarted &&
            restoreResult.result.items.any { it.state == RestoreItemState.Restored }
        ) {
            item {
                SlicesPanel(
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.secondaryContainer,
                ) {
                    Text(
                        text = stringResource(R.string.restore_app_not_started),
                        color = MaterialTheme.colorScheme.onSecondaryContainer,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
        }
        restoreResult?.let { completed ->
            items(completed.result.items, key = { it.archiveAccountId }) { item ->
                val name = state.manifest?.accounts?.firstOrNull {
                    it.archiveAccountId == item.archiveAccountId
                }?.name ?: item.archiveAccountId
                val status = when (item.state) {
                    RestoreItemState.Restored -> stringResource(R.string.restore_item_restored)
                    RestoreItemState.Failed -> stringResource(R.string.restore_item_failed)
                    RestoreItemState.Skipped -> stringResource(R.string.restore_item_skipped)
                }
                SlicesPanel(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = name,
                            style = MaterialTheme.typography.titleMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            text = status,
                            color = if (item.state == RestoreItemState.Restored) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.error
                            },
                        )
                    }
                }
            }
        }
        item {
            SlicesActionButton(
                text = stringResource(R.string.done),
                onClick = onDismiss,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun AccountIoRecoveryContent(
    state: BackupRestoreUiState,
    onIntent: (BackupUiIntent) -> Unit,
) {
    val pending = state.pendingAccountIoRecovery
    SlicesPanel(
        modifier = Modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.errorContainer,
    ) {
        Column(
            modifier = Modifier.padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(
                text = stringResource(R.string.account_io_recovery_title),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
            Text(
                text = stringResource(R.string.account_io_recovery_description),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
            state.accountIoStatuses.forEach { status ->
                Text(
                    text = status.packageName,
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
                Text(
                    text = stringResource(
                        R.string.account_io_recovery_status,
                        accountIoKindLabel(status.kind),
                        status.phase,
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
                if (pending?.ioToken == status.ioToken) {
                    Text(
                        text = stringResource(
                            R.string.account_io_recovery_confirm_message,
                            status.packageName,
                            accountIoKindLabel(status.kind),
                            status.phase,
                        ),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onErrorContainer,
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        SlicesActionButton(
                            text = stringResource(R.string.cancel),
                            onClick = { onIntent(BackupUiIntent.CancelAccountIoRecovery) },
                            enabled = !state.recoveringAccountIo,
                            primary = false,
                            modifier = Modifier.weight(1f),
                        )
                        SlicesActionButton(
                            text = stringResource(R.string.account_io_recovery_confirm),
                            onClick = { onIntent(BackupUiIntent.ConfirmAccountIoRecovery) },
                            enabled = !state.recoveringAccountIo,
                            danger = true,
                            modifier = Modifier.weight(1f),
                        )
                    }
                } else {
                    SlicesActionButton(
                        text = stringResource(R.string.account_io_recovery_request),
                        onClick = {
                            onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))
                        },
                        enabled = !state.recoveringAccountIo,
                        danger = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            state.accountIoRecoveryError?.let { error ->
                Text(
                    text = stringResource(
                        R.string.account_io_recovery_failed,
                        backupErrorMessage(error),
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
            }
        }
    }
}

@Composable
private fun accountIoKindLabel(kind: AccountIoKind): String = stringResource(
    when (kind) {
        AccountIoKind.Backup -> R.string.account_io_kind_backup
        AccountIoKind.Restore -> R.string.account_io_kind_restore
    },
)

@Composable
private fun PasswordField(
    value: String,
    label: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        visualTransformation = PasswordVisualTransformation(),
        modifier = modifier,
    )
}

@Composable
private fun BackupErrorText(code: String) {
    Text(
        text = backupErrorMessage(code),
        color = MaterialTheme.colorScheme.error,
        style = MaterialTheme.typography.bodyMedium,
    )
}

@Composable
private fun backupErrorMessage(code: String): String = stringResource(
    when (code) {
        "backup_password_required" -> R.string.backup_password_required
        "backup_auth_failed" -> R.string.backup_auth_failed
        "backup_invalid" -> R.string.backup_invalid
        "backup_incompatible", "identity_mismatch" -> R.string.backup_incompatible
        "insufficient_storage" -> R.string.insufficient_storage
        "io_busy" -> R.string.account_io_busy
        "invalid_request" -> R.string.backup_invalid_input
        "not_found" -> R.string.backup_target_missing
        else -> R.string.backup_restore_operation_failed
    },
)

private fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = arrayOf("KiB", "MiB", "GiB", "TiB")
    var value = bytes.toDouble()
    var unit = -1
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit += 1
    }
    return String.format(java.util.Locale.ROOT, "%.1f %s", value, units[unit])
}
