package com.uclone.slices.v2.backup

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.AccountIoKind
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.RestoreItemState
import com.uclone.slices.v2.runtime.RestoreTarget
import com.uclone.slices.v2.ui.Accent
import com.uclone.slices.v2.ui.AppButton
import com.uclone.slices.v2.ui.AppIcon
import com.uclone.slices.v2.ui.BASE_SLOT_ID
import com.uclone.slices.v2.ui.Banner
import com.uclone.slices.v2.ui.ButtonKind
import com.uclone.slices.v2.ui.EmptyState
import com.uclone.slices.v2.ui.Footnote
import com.uclone.slices.v2.ui.GroupCard
import com.uclone.slices.v2.ui.IconPlate
import com.uclone.slices.v2.ui.ListRow
import com.uclone.slices.v2.ui.ManagerScreen
import com.uclone.slices.v2.ui.Motion
import com.uclone.slices.v2.ui.SectionHeader
import com.uclone.slices.v2.ui.SlicesMenuItem
import com.uclone.slices.v2.ui.SlicesPasswordField
import com.uclone.slices.v2.ui.SlicesLinearProgress
import com.uclone.slices.v2.ui.SlicesPopupMenu
import com.uclone.slices.v2.ui.SlicesSpinner
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.Space
import com.uclone.slices.v2.ui.StatusChip
import com.uclone.slices.v2.ui.Surfaces
import com.uclone.slices.v2.ui.SwitchRow
import com.uclone.slices.v2.ui.Tabular
import com.uclone.slices.v2.ui.TextPrimary
import com.uclone.slices.v2.ui.TextSecondary
import com.uclone.slices.v2.ui.Tone
import com.uclone.slices.v2.ui.TypeScale
import com.uclone.slices.v2.ui.fadeThrough
import com.uclone.slices.v2.ui.onContainer
import com.uclone.slices.v2.ui.container
import com.uclone.slices.v2.ui.rememberReducedMotion
import com.uclone.slices.v2.ui.rememberStaggerPlay
import com.uclone.slices.v2.ui.rollingNumber
import com.uclone.slices.v2.ui.iosPush
import com.uclone.slices.v2.ui.spaceDisplayName
import com.uclone.slices.v2.ui.staggerIn
import java.text.DateFormat
import java.util.Date
import androidx.compose.material3.Text

/** Running job, or one of the pages. Running is drawn on its own full-page state. */
private const val RunningPane = -1

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
    val reduced = rememberReducedMotion()

    AnimatedContent(
        targetState = if (state.running) RunningPane else state.page.ordinal,
        transitionSpec = {
            iosPush(
                forward = targetState == RunningPane || targetState >= initialState,
                reduced = reduced,
            )
        },
        label = "backup_page",
        modifier = Modifier.fillMaxSize(),
    ) { pane ->
        if (pane == RunningPane) {
            RunningContent(state, contentPadding)
        } else when (BackupRestorePage.entries[pane]) {
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
                                stringResource(R.string.backup_one_account, spaceDisplayName(selectedSlot))
                            },
                            color = Accent,
                            fontSize = TypeScale.subhead,
                        )
                    }
                }
                if (selectedSlot == null) {
                    Text(
                        text = snapshot?.slots?.joinToString("、") { spaceDisplayName(it) }.orEmpty(),
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

@Composable
private fun RestorePasswordContent(
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

@Composable
private fun RestorePreviewContent(
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
                    androidx.compose.runtime.key(mapping.archiveAccountId) {
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
                ?.let(::spaceDisplayName)
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
                                text = spaceDisplayName(slot),
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

@Composable
private fun RunningContent(state: BackupRestoreUiState, contentPadding: PaddingValues) {
    val running = state.jobState as? BackupJobState.Running
    val reduced = rememberReducedMotion()
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Surfaces.page)
            .padding(contentPadding)
            .padding(Space.xl),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier.widthIn(max = 420.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .size(64.dp)
                    .clip(CircleShape)
                    .background(Tone.Info.container()),
                contentAlignment = Alignment.Center,
            ) {
                SlicesSpinner(size = 28.dp, color = Tone.Info.onContainer(), strokeWidth = 3.dp)
            }
            AnimatedContent(
                targetState = when (running?.kind) {
                    BackupJobKind.Backup -> stringResource(R.string.backup_running)
                    BackupJobKind.Inspect -> stringResource(R.string.inspect_running)
                    BackupJobKind.Restore -> stringResource(R.string.restore_running)
                    null -> stringResource(R.string.backup_restore_notification_text)
                },
                transitionSpec = fadeThrough(),
                label = "running-title",
                modifier = Modifier.padding(top = Space.md),
            ) { title ->
                Text(
                    text = title,
                    color = TextPrimary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center,
                )
            }
            if (running != null && running.bytesTotal > 0) {
                val fraction = (running.bytesDone.toDouble() / running.bytesTotal)
                    .coerceIn(0.0, 1.0)
                    .toFloat()
                val shown by animateFloatAsState(
                    targetValue = fraction,
                    animationSpec = if (reduced) snap() else Motion.spatial(),
                    label = "backup-progress",
                )
                SlicesLinearProgress(
                    progress = shown,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = Space.md),
                )
                AnimatedContent(
                    targetState = stringResource(R.string.data_progress, (fraction * 100).toInt()),
                    transitionSpec = rollingNumber(),
                    label = "backup-percent",
                    modifier = Modifier.padding(top = Space.xs),
                ) { text ->
                    Text(text = text, color = TextSecondary, fontSize = TypeScale.subhead, style = Tabular)
                }
            } else if (running != null && running.total > 0) {
                AnimatedContent(
                    targetState = stringResource(R.string.restore_progress, running.completed, running.total),
                    transitionSpec = rollingNumber(),
                    label = "restore-progress",
                    modifier = Modifier.padding(top = Space.xs),
                ) { text ->
                    Text(text = text, color = TextSecondary, fontSize = TypeScale.subhead, style = Tabular)
                }
            }
            Text(
                text = stringResource(R.string.do_not_force_stop),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = Space.sm),
            )
        }
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
                Banner(text = stringResource(R.string.restore_app_not_started), tone = Tone.Info)
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

@Composable
private fun AccountIoRecoveryContent(
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
private fun accountIoKindLabel(kind: AccountIoKind): String = stringResource(
    when (kind) {
        AccountIoKind.Backup -> R.string.account_io_kind_backup
        AccountIoKind.Restore -> R.string.account_io_kind_restore
    },
)

@Composable
private fun BackupErrorBanner(code: String, modifier: Modifier = Modifier) {
    Banner(text = backupErrorMessage(code), tone = Tone.Danger, modifier = modifier)
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

private fun formatAppVersion(versionName: String, versionCode: Long?): String =
    versionCode?.let { "$versionName ($it)" } ?: versionName
