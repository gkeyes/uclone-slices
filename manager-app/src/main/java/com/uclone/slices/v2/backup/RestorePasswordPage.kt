package com.uclone.slices.v2.backup

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.IconPlate
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.SlicesPasswordField
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale

@Composable
internal fun RestorePasswordContent(
    state: BackupRestoreUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
) {
    ManagerScreen(
        title = stringResource(R.string.enter_backup_password),
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") { Footnote(stringResource(R.string.backup_restore_subtitle)) }
        item(key = "password") {
            GroupCard(padded = true) {
                Row(
                    verticalAlignment = Alignment.Top,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    IconPlate(painterResource(R.drawable.ic_restore), Tone.Info)
                    Text(
                        text = stringResource(R.string.encrypted_backup_description),
                        color = TextSecondary,
                        fontSize = TypeScale.subhead,
                        lineHeight = 20.sp,
                        modifier = Modifier.weight(1f),
                    )
                }
                SlicesPasswordField(
                    value = state.password,
                    label = stringResource(R.string.backup_password),
                    onValueChange = { onIntent(BackupUiIntent.PasswordChanged(it)) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.md),
                )
            }
        }
        state.errorCode?.let { code ->
            item(key = "error") { BackupErrorBanner(code, Modifier.animateItem()) }
        }
        item(key = "decrypt") {
            AppButton(
                text = stringResource(R.string.decrypt_and_preview),
                onClick = { onIntent(BackupUiIntent.RetryRestorePassword) },
                enabled = state.password.isNotEmpty(),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter, vertical = Space.xs),
            )
        }
    }
}
