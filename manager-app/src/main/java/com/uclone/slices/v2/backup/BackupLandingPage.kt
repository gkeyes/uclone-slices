package com.uclone.slices.v2.backup

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.AppIcon
import com.uclone.slices.v2.ui.EmptyState
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.ListRow
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.SectionHeader
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.TypeScale
import com.uclone.slices.v2.ui.rememberStaggerPlay
import com.uclone.slices.v2.ui.staggerIn

@Composable
internal fun LandingContent(
    state: BackupRestoreUiState,
    slotsState: SlotsUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
    onChooseBackup: (String) -> Unit,
    onChooseRestore: () -> Unit,
) {
    val eligiblePackages = slotsState.packages.filter { it.runtimeReady && it.bindingState == BindingState.Ready }
    val play = rememberStaggerPlay("backup_landing")
    ManagerScreen(
        title = stringResource(R.string.backup_restore_title),
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") { Footnote(stringResource(R.string.backup_restore_subtitle)) }
        if (state.accountIoStatuses.isNotEmpty() || state.accountIoRecoveryError != null) {
            item(key = "recovery") {
                AccountIoRecoveryContent(state, onIntent, Modifier.animateItem())
            }
        }
        item(key = "restore_header") {
            SectionHeader(
                text = stringResource(R.string.restore_from_file),
                modifier = Modifier.staggerIn(0, play),
            )
        }
        item(key = "restore") {
            GroupCard(padded = true, modifier = Modifier.staggerIn(1, play)) {
                Text(
                    text = stringResource(R.string.restore_from_file_description),
                    color = TextSecondary,
                    fontSize = TypeScale.subhead,
                    lineHeight = 20.sp,
                )
                AppButton(
                    text = stringResource(R.string.choose_backup_file),
                    onClick = onChooseRestore,
                    enabled = slotsState.operationsAllowed,
                    icon = painterResource(R.drawable.ic_restore),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.md),
                )
            }
        }
        item(key = "backup_header") {
            SectionHeader(
                text = stringResource(R.string.create_account_backup),
                modifier = Modifier.staggerIn(2, play),
            )
        }
        if (eligiblePackages.isEmpty()) {
            item(key = "backup_empty") {
                GroupCard(modifier = Modifier.staggerIn(3, play)) {
                    EmptyState(
                        title = stringResource(R.string.no_backup_apps),
                        icon = painterResource(R.drawable.ic_backup),
                    )
                }
            }
        } else {
            item(key = "backup_apps") {
                GroupCard(modifier = Modifier.staggerIn(3, play)) {
                    eligiblePackages.forEach { snapshot ->
                        val app = slotsState.installedApps.firstOrNull {
                            it.packageName == snapshot.packageName
                        }
                        val label = app?.label ?: snapshot.packageName
                        ListRow(
                            title = label,
                            summary = stringResource(R.string.backup_account_count, snapshot.slots.size),
                            leading = { AppIcon(label, app?.icon, 36.dp) },
                            onClick = { onChooseBackup(snapshot.packageName) },
                            enabled = slotsState.operationsAllowed,
                        )
                    }
                }
            }
        }
        state.errorCode?.let { code ->
            item(key = "error") { BackupErrorBanner(code, Modifier.animateItem()) }
        }
    }
}
