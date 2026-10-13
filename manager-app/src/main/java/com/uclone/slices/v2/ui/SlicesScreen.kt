package com.uclone.slices.v2.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.backup.BackupRestoreScreen
import com.uclone.slices.v2.backup.BackupRestoreUiState
import com.uclone.slices.v2.backup.BackupUiIntent
import kotlinx.coroutines.delay
import top.yukonga.miuix.kmp.basic.Scaffold
import top.yukonga.miuix.kmp.basic.Text

@Composable
internal fun SlicesScreen(
    state: SlotsUiState,
    backupState: BackupRestoreUiState,
    onIntent: (UiIntent) -> Unit,
    onBackupIntent: (BackupUiIntent) -> Unit,
    suggestedBackupName: () -> String,
    uiPreferences: ManagerUiPreferences,
) {
    BackHandler(
        enabled = state.destination != ManagerDestination.Spaces &&
            !state.busy &&
            !backupState.running,
    ) {
        onIntent(UiIntent.NavigateBack)
    }
    val reduced = rememberReducedMotion()
    val pageOffset = with(LocalDensity.current) { Motion.PageOffset.roundToPx() }

    Box(modifier = Modifier.fillMaxSize()) {
        Scaffold(
            containerColor = Surfaces.page,
        ) { contentPadding ->
            AnimatedContent(
                targetState = state.destination,
                transitionSpec = {
                    sharedAxisX(
                        forward = targetState.depth() >= initialState.depth(),
                        offsetPx = pageOffset,
                        reduced = reduced,
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
                        uiPreferences = uiPreferences,
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
                    ManagerDestination.BackupRestore -> {
                        // Lives as long as this page is composed, so the window stays secure
                        // through the page's exit transition too.
                        SecureWindowEffect()
                        BackupRestoreScreen(
                            state = backupState,
                            slotsState = state,
                            contentPadding = contentPadding,
                            onBack = { onIntent(UiIntent.NavigateBack) },
                            onIntent = onBackupIntent,
                            suggestedBackupName = suggestedBackupName,
                        )
                    }
                    ManagerDestination.RuntimeStatus -> RuntimeStatusScreen(
                        state = state,
                        contentPadding = contentPadding,
                        onIntent = onIntent,
                    )
                }
            }
        }

        val configuring = rememberRetained(state.pendingConfigurationPackage)
        if (configuring != null) {
            ConfigureAppSheet(
                visible = state.pendingConfigurationPackage != null,
                packageName = configuring,
                installedApp = state.installedApps.firstOrNull { it.packageName == configuring },
                onConfirm = { onIntent(UiIntent.ConfirmConfigureApp) },
                onDismiss = { onIntent(UiIntent.DismissConfigureApp) },
            )
        }

        val recovery = rememberRetained(state.registrationRecovery)
        if (recovery != null) {
            RegistrationRecoverySheet(
                visible = state.registrationRecovery != null,
                recovery = recovery,
                onConfirmationChanged = {
                    onIntent(UiIntent.RegistrationRecoveryConfirmationChanged(it))
                },
                onConfirm = { onIntent(UiIntent.ConfirmRegistrationRecovery) },
                onDismiss = { onIntent(UiIntent.DismissRegistrationRecovery) },
            )
        }

        val rename = rememberRetained(state.pendingRename)
        if (rename != null) {
            RenameSpaceSheet(
                visible = state.pendingRename != null,
                rename = rename,
                onNameChanged = { onIntent(UiIntent.RenameSlotNameChanged(it)) },
                onConfirm = { onIntent(UiIntent.ConfirmRenameSlot) },
                onDismiss = { onIntent(UiIntent.DismissRenameSlot) },
            )
        }

        val deletion = rememberRetained(state.pendingDelete)
        if (deletion != null) {
            DeleteSpaceSheet(
                visible = state.pendingDelete != null,
                deletion = deletion,
                onConfirm = { onIntent(UiIntent.ConfirmDeleteSlot) },
                onDismiss = { onIntent(UiIntent.DismissDeleteSlot) },
            )
        }

        val unenrollment = rememberRetained(state.pendingUnenroll)
        if (unenrollment != null) {
            UnenrollAppSheet(
                visible = state.pendingUnenroll != null,
                unenrollment = unenrollment,
                onConfirmationChanged = {
                    onIntent(UiIntent.UnenrollConfirmationChanged(it))
                },
                onConfirm = { onIntent(UiIntent.ConfirmUnenrollApp) },
                onDismiss = { onIntent(UiIntent.DismissUnenrollApp) },
            )
        }

        val blocking = rememberRetained(state.operation.takeIf { it.blocksScreen })
        if (blocking != null) {
            OperationDialog(
                visible = state.operation.blocksScreen,
                operation = blocking,
            )
        }

        NoticeHost(
            notice = state.notice,
            onIntent = onIntent,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(start = Space.sm, top = Space.xs, end = Space.sm)
                .zIndex(10f),
        )
    }
}

/** Navigation depth: forward pushes slide in from the end, back slides the other way. */
private fun ManagerDestination.depth(): Int = when (this) {
    ManagerDestination.Spaces -> 0
    ManagerDestination.AddApp,
    ManagerDestination.RuntimeStatus,
    ManagerDestination.PackageDetails,
    -> 1
    ManagerDestination.BackupRestore -> 2
}

@Composable
private fun RenameSpaceSheet(
    visible: Boolean,
    rename: RenameSpaceUi,
    onNameChanged: (String) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val normalized = rename.name.trim()
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.rename_space_title),
        summary = stringResource(R.string.rename_space_hint),
    ) {
        SlicesInputField(
            value = rename.name,
            onValueChange = onNameChanged,
            label = stringResource(R.string.space_name),
            modifier = Modifier.fillMaxWidth(),
        )
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.save),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = normalized.isNotEmpty() && normalized != rename.originalName,
        )
    }
}

@Composable
private fun DeleteSpaceSheet(
    visible: Boolean,
    deletion: DeleteSpaceUi,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.delete_space_title),
        summary = stringResource(R.string.delete_space_message, deletion.name),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(
                text = stringResource(R.string.delete_space_data_warning),
                tone = Tone.Danger,
                gutter = false,
            )
            SheetNote(stringResource(R.string.delete_space_current_preserved))
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.delete_space_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            destructive = true,
        )
    }
}

@Composable
private fun UnenrollAppSheet(
    visible: Boolean,
    unenrollment: UnenrollAppUi,
    onConfirmationChanged: (Boolean) -> Unit,
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
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.unenroll_app_title),
        summary = stringResource(
            R.string.unenroll_app_message,
            unenrollment.appLabel,
            unenrollment.packageName,
        ),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(text = affectedSummary, tone = Tone.Danger, gutter = false)
            SheetNote(stringResource(R.string.unenroll_base_preserved))
            SwitchRow(
                title = stringResource(
                    if (unenrollment.affectedSpaces.isEmpty()) {
                        R.string.unenroll_confirmation_toggle_no_spaces
                    } else {
                        R.string.unenroll_confirmation_toggle
                    },
                ),
                checked = unenrollment.confirmed,
                onCheckedChange = { if (visible) onConfirmationChanged(it) },
                titleColor = Tone.Danger.onContainer(),
                horizontalPadding = Space.xxs,
            )
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.unenroll_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = unenrollment.confirmed,
            destructive = true,
        )
    }
}

@Composable
private fun ConfigureAppSheet(
    visible: Boolean,
    packageName: String,
    installedApp: InstalledApp?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val appLabel = installedApp?.label ?: packageName
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.configure_app_title),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(Radius.row))
                .background(Surfaces.control)
                .padding(Space.sm),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            AppIcon(appLabel, installedApp?.icon, 44.dp)
            Column(Modifier.weight(1f)) {
                Text(
                    text = appLabel,
                    color = TextPrimary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = packageName,
                    color = TextSecondary,
                    fontSize = TypeScale.caption,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.height(Space.sm))
        SheetNote(stringResource(R.string.configure_app_message))
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.configure_app),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
        )
    }
}

@Composable
private fun RegistrationRecoverySheet(
    visible: Boolean,
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
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.rebind_title),
        summary = stringResource(R.string.rebind_message, recovery.packageName),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            Banner(text = affectedSummary, tone = Tone.Info, gutter = false)
            SheetNote(stringResource(R.string.rebind_data_preserved))
            SlicesInputField(
                value = recovery.confirmation,
                onValueChange = { if (visible) onConfirmationChanged(it) },
                label = stringResource(R.string.rebind_confirmation_label),
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                text = stringResource(R.string.rebind_confirmation_hint),
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
                modifier = Modifier.padding(horizontal = Space.xxs),
            )
        }
        SheetActions(
            cancelText = stringResource(R.string.cancel),
            confirmText = stringResource(R.string.rebind_confirm),
            onCancel = { if (visible) onDismiss() },
            onConfirm = { if (visible) onConfirm() },
            confirmEnabled = recovery.confirmation == BINDING_CONFIRMATION,
        )
    }
}

/** Secondary paragraph inside a sheet. */
@Composable
private fun SheetNote(text: String) {
    Text(
        text = text,
        color = TextSecondary,
        fontSize = TypeScale.subhead,
        lineHeight = 20.sp,
        modifier = Modifier.padding(horizontal = Space.xxs),
    )
}

@Composable
private fun OperationDialog(visible: Boolean, operation: OperationUiState) {
    val message = when (operation) {
        OperationUiState.Idle -> ""
        OperationUiState.Refreshing -> stringResource(R.string.operation_refreshing)
        is OperationUiState.PreparingConfiguration ->
            stringResource(R.string.operation_preparing_configuration)
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
        OperationUiState.RestartingRuntime ->
            stringResource(R.string.operation_restarting_runtime)
    }
    CenterDialog(visible = visible) {
        Box(
            Modifier
                .size(64.dp)
                .clip(CircleShape)
                .background(Tone.Info.container()),
            contentAlignment = Alignment.Center,
        ) {
            SlicesSpinner(size = 28.dp, color = Tone.Info.onContainer(), strokeWidth = 3.dp)
        }
        Spacer(Modifier.height(Space.md))
        AnimatedContent(message, transitionSpec = fadeThrough(), label = "operation-title") {
            Text(
                text = it,
                color = TextPrimary,
                fontSize = TypeScale.headline,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(Space.xs))
        Text(
            text = stringResource(R.string.operation_wait_hint),
            color = TextSecondary,
            fontSize = TypeScale.subhead,
            lineHeight = 20.sp,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * Runtime problem card shown on top of lists while the Runtime is unavailable or incompatible: a
 * full-width row with a tonal status plate that opens the Runtime page.
 */
@Composable
internal fun RuntimeStatusCard(
    runtimeReady: Boolean,
    runtimeCompatible: Boolean,
    busy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tone = when {
        busy -> Tone.Info
        runtimeReady && runtimeCompatible -> Tone.Success
        else -> Tone.Warning
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
    GroupCard(modifier = modifier, color = tone.container()) {
        ListRow(
            title = title,
            summary = description,
            icon = painterResource(R.drawable.ic_layers),
            iconTone = tone,
            leading = {
                Box(
                    Modifier
                        .size(36.dp)
                        .clip(RoundedCornerShape(Radius.small))
                        .background(Surfaces.card.copy(alpha = .6f)),
                    contentAlignment = Alignment.Center,
                ) {
                    StatusDot(tone)
                }
            },
            onClick = onClick,
            enabled = enabled,
        )
    }
}

@Composable
private fun NoticeHost(
    notice: UiNotice?,
    onIntent: (UiIntent) -> Unit,
    modifier: Modifier = Modifier,
) {
    LaunchedEffect(notice) {
        val current = notice ?: return@LaunchedEffect
        delay(
            when {
                isSuccessNotice(current) -> 3_200L
                isRetryNotice(current) -> 5_200L
                else -> 4_400L
            },
        )
        onIntent(UiIntent.DismissNotice)
    }
    val reduced = rememberReducedMotion()

    AnimatedContent(
        targetState = notice,
        transitionSpec = {
            if (reduced) {
                ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
            } else {
                ContentTransform(
                    targetContentEnter = slideInVertically(Motion.offset) { -it - 24 } +
                        fadeIn(Motion.effects()),
                    initialContentExit = slideOutVertically(Motion.exit()) { -it / 2 } +
                        fadeOut(Motion.exit()),
                    sizeTransform = SizeTransform(clip = false) { _, _ -> Motion.size },
                )
            }
        },
        contentAlignment = Alignment.TopCenter,
        label = "floating_notice",
        modifier = modifier,
    ) { current ->
        current?.let {
            FloatingNoticeCard(
                notice = it,
                onIntent = onIntent,
            )
        }
    }
}

@Composable
private fun FloatingNoticeCard(
    notice: UiNotice,
    onIntent: (UiIntent) -> Unit,
) {
    val success = isSuccessNotice(notice)
    val retry = isRetryNotice(notice)
    val tone = if (success) Tone.Success else Tone.Danger
    val headline = noticeHeadline(notice)
    val supporting = if (notice is UiNotice.SpaceActivated && notice.switched) {
        stringResource(R.string.success_switch_launch_hint)
    } else {
        null
    }
    val shape = RoundedCornerShape(Radius.row + Space.xxs)
    Row(
        modifier = Modifier
            .widthIn(max = 560.dp)
            .fillMaxWidth()
            .graphicsLayer {
                shadowElevation = 6.dp.toPx()
                this.shape = shape
                clip = true
            }
            .background(Surfaces.raised)
            .border(0.5.dp, Surfaces.hairline, shape)
            .padding(
                start = 10.dp,
                top = 10.dp,
                end = if (retry) Space.xxs else 14.dp,
                bottom = 10.dp,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(36.dp)
                .clip(RoundedCornerShape(Radius.small))
                .background(tone.container()),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = if (success) "✓" else "!",
                color = tone.onContainer(),
                fontSize = TypeScale.headline,
                fontWeight = FontWeight.Bold,
            )
        }
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(start = Space.sm),
        ) {
            Text(
                text = headline,
                color = TextPrimary,
                fontSize = TypeScale.subhead,
                fontWeight = FontWeight.SemiBold,
                lineHeight = 20.sp,
            )
            supporting?.let {
                Text(
                    text = it,
                    color = TextSecondary,
                    fontSize = TypeScale.caption,
                    lineHeight = 17.sp,
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
        if (retry) {
            AppButton(
                text = stringResource(R.string.retry),
                onClick = { onIntent(UiIntent.Refresh) },
                kind = ButtonKind.Text,
                compact = true,
            )
        }
    }
}

private fun isSuccessNotice(notice: UiNotice): Boolean =
    notice is UiNotice.RuntimeRestarted ||
        notice is UiNotice.ConfigurationCompleted ||
        notice is UiNotice.ConfigurationRebound ||
        notice is UiNotice.SpaceCreated ||
        notice is UiNotice.SpaceActivated ||
        notice is UiNotice.SpaceRenamed ||
        notice is UiNotice.SpaceDeleted ||
        notice is UiNotice.AppUnenrolled

private fun isRetryNotice(notice: UiNotice): Boolean =
    notice is UiNotice.LocalAppsUnavailable ||
        notice is UiNotice.RuntimeUnavailable ||
        notice is UiNotice.MissingEntity ||
        notice is UiNotice.StateChanged ||
        notice is UiNotice.OperationFailed

@Composable
private fun noticeHeadline(notice: UiNotice): String =
    if (notice is UiNotice.SpaceActivated && notice.switched) {
        stringResource(R.string.success_switched_title, notice.name)
    } else {
        noticeMessage(notice)
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
    UiNotice.AccountIoBusy -> stringResource(R.string.error_account_io_busy)
    UiNotice.StorageInsufficient -> stringResource(R.string.error_storage_insufficient)
    UiNotice.RuntimeRestartFailed -> stringResource(R.string.error_runtime_restart_failed)
    UiNotice.RuntimeRestarted -> stringResource(R.string.success_runtime_restarted)
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
