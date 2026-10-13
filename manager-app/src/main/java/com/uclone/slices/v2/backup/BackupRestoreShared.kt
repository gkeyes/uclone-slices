package com.uclone.slices.v2.backup

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.AccountIoKind
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.Banner
import com.uclone.slices.v2.ui.ButtonKind
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.IconPlate
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.TextPrimary
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale

@Composable
internal fun AccountIoRecoveryContent(
    state: BackupRestoreUiState,
    onIntent: (BackupUiIntent) -> Unit,
    modifier: Modifier = Modifier,
) {
    val pending = state.pendingAccountIoRecovery
    GroupCard(modifier = modifier, padded = true) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            IconPlate(painterResource(R.drawable.ic_restore), Tone.Danger)
            Text(
                text = stringResource(R.string.account_io_recovery_title),
                color = TextPrimary,
                fontSize = TypeScale.headline,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
        }
        Text(
            text = stringResource(R.string.account_io_recovery_description),
            color = TextSecondary,
            fontSize = TypeScale.subhead,
            lineHeight = 20.sp,
            modifier = Modifier.padding(top = Space.sm),
        )
        state.accountIoStatuses.forEach { status ->
            Spacer(Modifier.height(Space.md))
            Text(
                text = status.packageName,
                color = TextPrimary,
                fontSize = TypeScale.body,
                fontWeight = FontWeight.Medium,
            )
            Text(
                text = stringResource(
                    R.string.account_io_recovery_status,
                    accountIoKindLabel(status.kind),
                    status.phase,
                ),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
            )
            if (pending?.ioToken == status.ioToken) {
                Banner(
                    text = stringResource(
                        R.string.account_io_recovery_confirm_message,
                        status.packageName,
                        accountIoKindLabel(status.kind),
                        status.phase,
                    ),
                    tone = Tone.Danger,
                    gutter = false,
                    modifier = Modifier.padding(top = Space.sm),
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.sm),
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    AppButton(
                        text = stringResource(R.string.cancel),
                        onClick = { onIntent(BackupUiIntent.CancelAccountIoRecovery) },
                        enabled = !state.recoveringAccountIo,
                        kind = ButtonKind.Secondary,
                        rejectHaptic = true,
                        modifier = Modifier.weight(1f),
                    )
                    AppButton(
                        text = stringResource(R.string.account_io_recovery_confirm),
                        onClick = { onIntent(BackupUiIntent.ConfirmAccountIoRecovery) },
                        enabled = !state.recoveringAccountIo,
                        loading = state.recoveringAccountIo,
                        kind = ButtonKind.Destructive,
                        modifier = Modifier.weight(1f),
                    )
                }
            } else {
                AppButton(
                    text = stringResource(R.string.account_io_recovery_request),
                    onClick = {
                        onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))
                    },
                    enabled = !state.recoveringAccountIo,
                    kind = ButtonKind.Destructive,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.sm),
                )
            }
        }
        state.accountIoRecoveryError?.let { error ->
            Banner(
                text = stringResource(
                    R.string.account_io_recovery_failed,
                    backupErrorMessage(error),
                ),
                tone = Tone.Danger,
                gutter = false,
                modifier = Modifier.padding(top = Space.md),
            )
        }
    }
}

@Composable
internal fun accountIoKindLabel(kind: AccountIoKind): String = stringResource(
    when (kind) {
        AccountIoKind.Backup -> R.string.account_io_kind_backup
        AccountIoKind.Restore -> R.string.account_io_kind_restore
    },
)

@Composable
internal fun BackupErrorBanner(code: String, modifier: Modifier = Modifier) {
    Banner(text = backupErrorMessage(code), tone = Tone.Danger, modifier = modifier)
}

@Composable
internal fun backupErrorMessage(code: String): String = stringResource(backupErrorMessageRes(code))
