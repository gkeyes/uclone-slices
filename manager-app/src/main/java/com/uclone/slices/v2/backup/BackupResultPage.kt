package com.uclone.slices.v2.backup

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.RestoreItemState
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.Banner
import com.uclone.slices.v2.ui.EmptyState
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.ListRow
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.StatusChip
import com.uclone.slices.v2.ui.Tone

@Composable
internal fun ResultContent(
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
            summary = if (result.manifest.profile != null) {
                stringResource(
                    R.string.profile_backup_complete_summary,
                    result.manifest.accounts.size,
                    formatBytes(result.archiveSize),
                    formatBytes(result.manifest.profile.excludedLogicalSize),
                )
            } else {
                stringResource(
                    R.string.backup_complete_summary,
                    result.manifest.accounts.size,
                    formatBytes(result.archiveSize),
                )
            }
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
    ManagerScreen(
        title = title,
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") { Footnote(stringResource(R.string.backup_restore_subtitle)) }
        if (state.accountIoStatuses.isNotEmpty() || state.accountIoRecoveryError != null) {
            item(key = "recovery") {
                AccountIoRecoveryContent(state, onIntent, Modifier.animateItem())
            }
        }
        item(key = "summary") {
            GroupCard {
                EmptyState(
                    title = title,
                    message = summary,
                    icon = painterResource(if (success) R.drawable.ic_backup else R.drawable.ic_restore),
                    tone = if (success) Tone.Success else Tone.Danger,
                )
            }
        }
        if (
            restoreResult != null &&
            !restoreResult.result.appStarted &&
            restoreResult.result.items.any { it.state == RestoreItemState.Restored }
        ) {
            item(key = "not_started") {
                Banner(
                    text = stringResource(R.string.restore_app_not_started),
                    tone = Tone.Info,
                    animateText = false,
                )
            }
        }
        restoreResult?.let { completed ->
            item(key = "items") {
                GroupCard {
                    completed.result.items.forEach { item ->
                        val name = state.manifest?.accounts?.firstOrNull {
                            it.archiveAccountId == item.archiveAccountId
                        }?.name ?: item.archiveAccountId
                        val (status, tone) = when (item.state) {
                            RestoreItemState.Restored ->
                                stringResource(R.string.restore_item_restored) to Tone.Success
                            RestoreItemState.Failed ->
                                stringResource(R.string.restore_item_failed) to Tone.Danger
                            RestoreItemState.Skipped ->
                                stringResource(R.string.restore_item_skipped) to Tone.Warning
                        }
                        ListRow(
                            title = name,
                            trailing = { StatusChip(text = status, tone = tone) },
                        )
                    }
                }
            }
        }
        item(key = "done") {
            AppButton(
                text = stringResource(R.string.done),
                onClick = onDismiss,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter, vertical = Space.xs),
            )
        }
    }
}
