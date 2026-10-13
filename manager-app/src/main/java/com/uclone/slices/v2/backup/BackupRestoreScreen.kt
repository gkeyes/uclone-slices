package com.uclone.slices.v2.backup

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.iosPush
import com.uclone.slices.v2.ui.rememberReducedMotion

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
