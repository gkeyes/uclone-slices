package com.uclone.slices.v2.backup

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.RestoreTarget
import com.uclone.slices.v2.ui.Accent
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.AppIcon
import com.uclone.slices.v2.ui.BASE_SLOT_ID
import com.uclone.slices.v2.ui.Banner
import com.uclone.slices.v2.ui.ButtonKind
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.ListRow
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.SectionHeader
import com.uclone.slices.v2.ui.SlicesMenuItem
import com.uclone.slices.v2.ui.SlicesPopupMenu
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.SwitchRow
import com.uclone.slices.v2.ui.Tabular
import com.uclone.slices.v2.ui.TextPrimary
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale
import com.uclone.slices.v2.ui.localizedSpaceName
import com.uclone.slices.v2.ui.onContainer
import java.text.DateFormat
import java.util.Date

@Composable
internal fun RestorePreviewContent(
    state: BackupRestoreUiState,
    contentPadding: PaddingValues,
    onBack: () -> Unit,
    onIntent: (BackupUiIntent) -> Unit,
) {
    val manifest = state.manifest ?: return
    ManagerScreen(
        title = stringResource(R.string.restore_preview),
        contentPadding = contentPadding,
        onBack = onBack,
    ) {
        item(key = "subtitle") { Footnote(stringResource(R.string.backup_restore_subtitle)) }
        item(key = "archive") {
            val label = state.targetApp?.label ?: manifest.packageName
            GroupCard(padded = true) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    AppIcon(label, state.targetApp?.icon, 48.dp)
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
                            text = manifest.packageName,
                            color = TextSecondary,
                            fontSize = TypeScale.caption,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Text(
                    text = stringResource(
                        R.string.archive_summary,
                        manifest.accounts.size,
                        formatBytes(manifest.logicalSize),
                    ),
                    color = Accent,
                    fontSize = TypeScale.body,
                    fontWeight = FontWeight.Medium,
                    style = Tabular,
                    modifier = Modifier.padding(top = Space.md),
                )
                Column(
                    modifier = Modifier.padding(top = Space.xs),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    MetaLine(
                        stringResource(
                            R.string.archive_created,
                            DateFormat.getDateTimeInstance().format(Date(manifest.createdAtMillis)),
                        ),
                    )
                    MetaLine(
                        stringResource(
                            R.string.archive_origin,
                            formatAppVersion(manifest.appVersion, manifest.appVersionCode),
                            manifest.androidVersion,
                            manifest.device,
                        ),
                    )
                    MetaLine(
                        stringResource(
                            R.string.archive_target_version,
                            state.targetApp?.let {
                                formatAppVersion(it.versionName, it.versionCode)
                            }.orEmpty(),
                        ),
                    )
                }
            }
        }
        if (!state.signatureMatches) {
            item(key = "signature") {
                Banner(
                    text = stringResource(R.string.signature_mismatch_warning),
                    tone = Tone.Danger,
                    animateText = false,
                )
            }
        }
        item(key = "profile") {
            val preserveExisting = stringResource(R.string.profile_restore_preserve_existing)
            val newAccount = stringResource(R.string.profile_restore_new_account)
            val unavailable = stringResource(R.string.profile_restore_unavailable)
            val fullArchive = stringResource(R.string.full_archive_restore_resources)
            val lines = buildList {
                when {
                    state.restoreProfile != null -> {
                        if (state.mappings.any { it.target is RestoreTarget.Existing }) {
                            add(preserveExisting)
                        }
                        if (state.mappings.any { it.target is RestoreTarget.New }) {
                            add(newAccount)
                        }
                    }
                    state.profileUnavailable -> add(unavailable)
                    else -> add(fullArchive)
                }
            }
            if (lines.isNotEmpty()) {
                Banner(text = lines.joinToString("\n"), tone = Tone.Info)
            }
        }
        item(key = "mapping_header") {
            SectionHeader(stringResource(R.string.restore_account_mapping))
        }
        item(key = "mappings") {
            GroupCard {
                state.mappings.forEach { mapping ->
                    key(mapping.archiveAccountId) {
                        MappingRow(mapping, state, onIntent)
                    }
                }
            }
        }
        item(key = "confirm") {
            GroupCard(padded = true) {
                Banner(
                    text = stringResource(
                        if (state.restoreProfile != null) {
                            R.string.restore_profile_overwrite_warning
                        } else {
                            R.string.restore_overwrite_warning
                        },
                    ),
                    tone = Tone.Danger,
                    gutter = false,
                )
                SwitchRow(
                    title = stringResource(
                        if (state.signatureMatches) {
                            R.string.restore_confirmation_toggle
                        } else {
                            R.string.restore_signature_mismatch_confirmation_toggle
                        },
                    ),
                    checked = state.destructiveConfirmed,
                    onCheckedChange = {
                        onIntent(BackupUiIntent.DestructiveConfirmationChanged(it))
                    },
                    titleColor = Tone.Danger.onContainer(),
                    horizontalPadding = Space.xxs,
                    modifier = Modifier.padding(top = Space.xs),
                )
            }
        }
        state.errorCode?.let { code ->
            item(key = "error") { BackupErrorBanner(code, Modifier.animateItem()) }
        }
        item(key = "start") {
            AppButton(
                text = stringResource(R.string.start_restore),
                onClick = { onIntent(BackupUiIntent.StartRestore) },
                enabled = state.destructiveConfirmed,
                kind = ButtonKind.Destructive,
                icon = painterResource(R.drawable.ic_restore),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter, vertical = Space.xs),
            )
        }
    }
}

@Composable
private fun MetaLine(text: String) {
    Text(
        text = text,
        color = TextSecondary,
        fontSize = TypeScale.caption,
        lineHeight = 18.sp,
        style = Tabular,
    )
}

@Composable
private fun MappingRow(
    mapping: RestoreMappingUi,
    state: BackupRestoreUiState,
    onIntent: (BackupUiIntent) -> Unit,
) {
    val showTargets = remember(mapping.archiveAccountId) { mutableStateOf(false) }
    val targetText = when (val target = mapping.target) {
        RestoreTarget.New -> stringResource(R.string.restore_as_new_account)
        is RestoreTarget.Existing -> if (target.slotId == BASE_SLOT_ID) {
            stringResource(R.string.system_original_space)
        } else {
            state.targetPackage?.slots?.firstOrNull { it.id == target.slotId }
                ?.let { localizedSpaceName(it) }
                ?: target.slotId
        }
    }
    val isSlot = mapping.archiveKind == ArchiveAccountKind.Slot
    ListRow(
        title = mapping.name,
        summary = if (mapping.archiveKind == ArchiveAccountKind.Base) {
            stringResource(R.string.base_restore_fixed)
        } else {
            stringResource(R.string.restore_target, targetText)
        },
        icon = painterResource(if (isSlot) R.drawable.ic_storage else R.drawable.ic_layers),
        iconTone = if (isSlot) Tone.Info else Tone.Neutral,
        trailing = if (isSlot) {
            {
                val newAccountText = stringResource(R.string.restore_as_new_account)
                val targets = buildList {
                    add(
                        SlicesMenuItem(
                            text = newAccountText,
                            checked = mapping.target == RestoreTarget.New,
                            onClick = {
                                onIntent(
                                    BackupUiIntent.RestoreMappingChanged(
                                        mapping.archiveAccountId,
                                        RestoreTarget.New,
                                    ),
                                )
                            },
                        ),
                    )
                    state.targetPackage?.slots?.filterNot { it.id == BASE_SLOT_ID }?.forEach { slot ->
                        add(
                            SlicesMenuItem(
                                text = localizedSpaceName(slot),
                                checked = mapping.target == RestoreTarget.Existing(slot.id),
                                onClick = {
                                    onIntent(
                                        BackupUiIntent.RestoreMappingChanged(
                                            mapping.archiveAccountId,
                                            RestoreTarget.Existing(slot.id),
                                        ),
                                    )
                                },
                            ),
                        )
                    }
                }
                Box {
                    AppButton(
                        text = stringResource(R.string.change_restore_target),
                        onClick = { showTargets.value = true },
                        kind = ButtonKind.Text,
                        compact = true,
                    )
                    SlicesPopupMenu(show = showTargets, items = targets)
                }
            }
        } else {
            null
        },
        summaryMaxLines = 2,
    )
}
