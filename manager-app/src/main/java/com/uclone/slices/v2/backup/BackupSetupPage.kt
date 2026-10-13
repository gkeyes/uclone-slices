package com.uclone.slices.v2.backup

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.ui.Accent
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.AppIcon
import com.uclone.slices.v2.ui.Banner
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.Motion
import com.uclone.slices.v2.ui.SectionHeader
import com.uclone.slices.v2.ui.SlicesPasswordField
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.SwitchRow
import com.uclone.slices.v2.ui.TextPrimary
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale
import com.uclone.slices.v2.ui.joinSpaceNames
import com.uclone.slices.v2.ui.localizedSpaceName
import com.uclone.slices.v2.ui.rememberReducedMotion

@Composable
internal fun BackupSetupContent(
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
    val reduced = rememberReducedMotion()
    ManagerScreen(
        title = stringResource(R.string.create_account_backup),
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") { Footnote(stringResource(R.string.backup_restore_subtitle)) }
        item(key = "target") {
            val label = app?.label ?: state.packageName.orEmpty()
            GroupCard(padded = true) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    AppIcon(label, app?.icon, 48.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            text = label,
                            color = TextPrimary,
                            fontSize = TypeScale.headline,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            text = if (selectedSlot == null) {
                                stringResource(R.string.backup_all_accounts)
                            } else {
                                stringResource(R.string.backup_one_account, localizedSpaceName(selectedSlot))
                            },
                            color = Accent,
                            fontSize = TypeScale.subhead,
                        )
                    }
                }
                if (selectedSlot == null) {
                    Text(
                        text = snapshot?.slots
                            ?.map { localizedSpaceName(it) }
                            ?.let { joinSpaceNames(it) }
                            .orEmpty(),
                        color = TextSecondary,
                        fontSize = TypeScale.caption,
                        lineHeight = 18.sp,
                        modifier = Modifier.padding(top = Space.sm),
                    )
                }
            }
        }
        item(key = "password_header") {
            SectionHeader(stringResource(R.string.password_protection))
        }
        item(key = "password") {
            GroupCard {
                SwitchRow(
                    title = stringResource(R.string.password_protection),
                    summary = stringResource(R.string.password_protection_default),
                    checked = state.passwordProtected,
                    onCheckedChange = { onIntent(BackupUiIntent.PasswordProtectionChanged(it)) },
                )
                AnimatedVisibility(
                    visible = state.passwordProtected,
                    enter = if (reduced) EnterTransition.None else expandVertically(Motion.size) + fadeIn(Motion.effects()),
                    exit = if (reduced) ExitTransition.None else shrinkVertically(Motion.size) + fadeOut(Motion.effects()),
                ) {
                    Column(
                        modifier = Modifier.padding(start = Space.md, end = Space.md, bottom = Space.xs),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        SlicesPasswordField(
                            value = state.password,
                            label = stringResource(R.string.backup_password),
                            onValueChange = { onIntent(BackupUiIntent.PasswordChanged(it)) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        SlicesPasswordField(
                            value = state.passwordConfirmation,
                            label = stringResource(R.string.confirm_backup_password),
                            onValueChange = {
                                onIntent(BackupUiIntent.PasswordConfirmationChanged(it))
                            },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
                Banner(
                    text = stringResource(
                        if (state.passwordProtected) {
                            R.string.password_loss_warning
                        } else {
                            R.string.unencrypted_backup_warning
                        },
                    ),
                    tone = if (state.passwordProtected) Tone.Warning else Tone.Danger,
                    modifier = Modifier.padding(start = Space.md, end = Space.md, top = Space.xs, bottom = Space.md),
                    gutter = false,
                )
            }
        }
        item(key = "exclusions") { Footnote(stringResource(R.string.backup_scope_exclusions)) }
        if (state.backupProfile != null) {
            item(key = "profile") {
                Banner(
                    text = stringResource(R.string.profile_backup_automatic),
                    tone = Tone.Info,
                    modifier = Modifier.animateItem(),
                )
            }
        }
        state.errorCode?.let { code ->
            item(key = "error") { BackupErrorBanner(code, Modifier.animateItem()) }
        }
        item(key = "start") {
            AppButton(
                text = stringResource(R.string.choose_save_location),
                onClick = onStart,
                enabled = snapshot != null &&
                    (!state.passwordProtected || (
                        state.password.isNotEmpty() &&
                            state.password == state.passwordConfirmation
                        )),
                icon = painterResource(R.drawable.ic_backup),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter, vertical = Space.xs),
            )
        }
    }
}
