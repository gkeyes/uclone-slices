package com.uclone.slices.v2.ui

import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SeedMode

enum class ManagerDestination {
    Spaces,
    AddApp,
    PackageDetails,
    BackupRestore,
    RuntimeStatus,
}

sealed interface OperationUiState {
    data object Idle : OperationUiState
    data object Refreshing : OperationUiState
    data class PreparingConfiguration(val packageName: String) : OperationUiState
    data class Configuring(val packageName: String) : OperationUiState
    data class OpeningPackage(val packageName: String) : OperationUiState
    data class CreatingSpace(
        val packageName: String,
        val spaceName: String,
    ) : OperationUiState
    data class ActivatingSpace(
        val packageName: String,
        val spaceName: String,
        val switched: Boolean,
        val slotId: String = "",
    ) : OperationUiState
    data class RenamingSpace(
        val packageName: String,
        val spaceName: String,
    ) : OperationUiState
    data class DeletingSpace(
        val packageName: String,
        val spaceName: String,
    ) : OperationUiState
    data class UnenrollingApp(
        val packageName: String,
        val appLabel: String,
    ) : OperationUiState
    data class SavingRebootLaunch(val packageName: String) : OperationUiState
    data class RebindingConfiguration(val packageName: String) : OperationUiState
    data object RestartingRuntime : OperationUiState
}

/**
 * Only long or destructive operations cover the screen. Switching, renaming, toggles and
 * opening a page show their progress where they were started.
 */
internal val OperationUiState.blocksScreen: Boolean
    get() = when (this) {
        is OperationUiState.PreparingConfiguration,
        is OperationUiState.Configuring,
        is OperationUiState.CreatingSpace,
        is OperationUiState.DeletingSpace,
        is OperationUiState.UnenrollingApp,
        is OperationUiState.RebindingConfiguration,
        OperationUiState.RestartingRuntime,
        -> true
        OperationUiState.Idle,
        OperationUiState.Refreshing,
        is OperationUiState.OpeningPackage,
        is OperationUiState.ActivatingSpace,
        is OperationUiState.RenamingSpace,
        is OperationUiState.SavingRebootLaunch,
        -> false
    }

sealed interface UiNotice {
    data object LocalAppsUnavailable : UiNotice
    data object RuntimeUnavailable : UiNotice
    data object InvalidInput : UiNotice
    data object MissingEntity : UiNotice
    data object StateChanged : UiNotice
    data object OperationFailed : UiNotice
    data object IdentityProtected : UiNotice
    data object SigningUnavailable : UiNotice
    data object RuntimeVersionMismatch : UiNotice
    data object AccountIoBusy : UiNotice
    data object StorageInsufficient : UiNotice
    data object RuntimeRestartFailed : UiNotice
    data object RuntimeRestarted : UiNotice
    data object ConfigurationCompleted : UiNotice
    data object ConfigurationRebound : UiNotice
    data class SpaceCreated(val name: String) : UiNotice
    data class SpaceActivated(
        val name: String,
        val switched: Boolean,
    ) : UiNotice
    data class SpaceRenamed(val name: String) : UiNotice
    data class SpaceDeleted(val name: String) : UiNotice
    data class AppUnenrolled(val appLabel: String) : UiNotice
}

data class RegistrationRecoveryUi(
    val packageName: String,
    val affectedSpaces: List<String>,
    val confirmation: String = "",
)

data class RenameSpaceUi(
    val packageName: String,
    val slotId: String,
    val originalName: String,
    val name: String = originalName,
)

data class DeleteSpaceUi(
    val packageName: String,
    val slotId: String,
    val name: String,
)

data class UnenrollAppUi(
    val packageName: String,
    val appLabel: String,
    val affectedSpaces: List<String>,
    val confirmed: Boolean = false,
)

/**
 * A notice as shown at the top of the screen. Every post gets a new [id], so the same notice raised
 * twice (for example a retry that fails again) is shown and timed again instead of being treated
 * as unchanged state.
 */
data class PostedNotice(val id: Long, val notice: UiNotice)

data class SlotsUiState(
    val destination: ManagerDestination = ManagerDestination.Spaces,
    val backupReturnDestination: ManagerDestination = ManagerDestination.Spaces,
    val runtimeReady: Boolean = false,
    val runtimeCompatible: Boolean = false,
    val buildId: String = "",
    val initialLoadComplete: Boolean = false,
    val configuredAccountsExpanded: Boolean = true,
    val installedApps: List<InstalledApp> = emptyList(),
    val appCatalogLoading: Boolean = false,
    val appCatalogLoaded: Boolean = false,
    val packages: List<PackageSnapshot> = emptyList(),
    val selected: PackageSnapshot? = null,
    val seed: SeedMode = SeedMode.Blank,
    val operation: OperationUiState = OperationUiState.Idle,
    val posted: PostedNotice? = null,
    val pendingConfigurationPackage: String? = null,
    val registrationRecovery: RegistrationRecoveryUi? = null,
    val pendingRename: RenameSpaceUi? = null,
    val pendingDelete: DeleteSpaceUi? = null,
    val pendingUnenroll: UnenrollAppUi? = null,
    val runtimeLog: String? = null,
) {
    val notice: UiNotice?
        get() = posted?.notice
    val busy: Boolean
        get() = operation !is OperationUiState.Idle
    val operationsAllowed: Boolean
        get() = runtimeReady && runtimeCompatible
}

sealed interface UiIntent {
    data object Refresh : UiIntent
    data object AppResumed : UiIntent
    data object RestartRuntime : UiIntent
    data object LoadRuntimeLog : UiIntent
    data object OpenAddApps : UiIntent
    data object OpenRuntimeStatus : UiIntent
    data class OpenBackupRestore(
        val packageName: String? = null,
        val slotId: String? = null,
    ) : UiIntent
    data class OpenPackage(val packageName: String) : UiIntent
    data class SetConfiguredAccountsExpanded(val expanded: Boolean) : UiIntent
    data class SetLaunchAfterReboot(
        val packageName: String,
        val enabled: Boolean,
    ) : UiIntent
    data class QuickActivateSlot(val packageName: String, val slotId: String) : UiIntent
    data object NavigateBack : UiIntent
    data class RequestConfigureApp(val packageName: String) : UiIntent
    data object ConfirmConfigureApp : UiIntent
    data object DismissConfigureApp : UiIntent
    data class SeedChanged(val value: SeedMode) : UiIntent
    data class CreateSlot(val name: String) : UiIntent
    data class ActivateSlot(val slotId: String) : UiIntent
    data class RequestRenameSlot(val slotId: String) : UiIntent
    data class RenameSlotNameChanged(val value: String) : UiIntent
    data object ConfirmRenameSlot : UiIntent
    data object DismissRenameSlot : UiIntent
    data class RequestDeleteSlot(val slotId: String) : UiIntent
    data object ConfirmDeleteSlot : UiIntent
    data object DismissDeleteSlot : UiIntent
    data class RequestUnenrollApp(val packageName: String) : UiIntent
    data class UnenrollConfirmationChanged(val confirmed: Boolean) : UiIntent
    data object ConfirmUnenrollApp : UiIntent
    data object DismissUnenrollApp : UiIntent
    data class DismissNotice(val id: Long) : UiIntent
    data object DismissRegistrationRecovery : UiIntent
    data class RegistrationRecoveryConfirmationChanged(val value: String) : UiIntent
    data object ConfirmRegistrationRecovery : UiIntent
}

/** The notice shown when a Runtime command on the spaces screens fails with [this] code. */
internal fun ErrorCode.toUiNotice(): UiNotice = when (this) {
    ErrorCode.InvalidRequest -> UiNotice.InvalidInput
    ErrorCode.NotFound -> UiNotice.MissingEntity
    ErrorCode.StateConflict -> UiNotice.StateChanged
    ErrorCode.IdentityMismatch -> UiNotice.IdentityProtected
    ErrorCode.OperationFailed -> UiNotice.OperationFailed
    ErrorCode.IoBusy -> UiNotice.AccountIoBusy
    ErrorCode.InsufficientStorage -> UiNotice.StorageInsufficient
    ErrorCode.BackupInvalid,
    ErrorCode.BackupPasswordRequired,
    ErrorCode.BackupAuthFailed,
    ErrorCode.BackupIncompatible,
    -> UiNotice.OperationFailed
}

internal const val BASE_SLOT_ID = "base"
internal const val BINDING_CONFIRMATION = "绑定"
