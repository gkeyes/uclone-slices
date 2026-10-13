package com.uclone.slices.v2.ui

import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SeedMode

/**
 * The Manager's screens as the view model reasons about them. Navigation itself is owned by the
 * Navigation-Compose back stack (ManagerNavHost.kt); the nav host reports the screen on top with
 * [UiIntent.DestinationChanged] and the view model asks for moves with [SlotsNavigation].
 */
enum class ManagerDestination {
    Spaces,
    AddApp,
    PackageDetails,
    BackupRestore,
    RuntimeStatus,
}

/** A move the view model asks the nav host to make; delivered once through `navigation`. */
sealed interface SlotsNavigation {
    /** Show a configured app's page above the spaces list (replacing the Add App page). */
    data class ToPackage(val packageName: String) : SlotsNavigation
    data object ToAddApps : SlotsNavigation
    data object ToRuntimeStatus : SlotsNavigation
    data class ToBackupRestore(
        val packageName: String? = null,
        val slotId: String? = null,
    ) : SlotsNavigation

    /** Pop back to the spaces list (the page shown no longer applies). */
    data object ToSpaces : SlotsNavigation
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
    /** Manual home order (package names) as saved; empty means the default order. */
    val homeAppOrder: List<String> = emptyList(),
    /** The order being edited while the home list is in reorder mode; null otherwise. */
    val reorderDraft: List<String>? = null,
) {
    val reordering: Boolean
        get() = reorderDraft != null
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

    /**
     * The nav host reports the screen now on top of the back stack; for the package page also
     * the package its route names (used to restore the page after process death).
     */
    data class DestinationChanged(
        val destination: ManagerDestination,
        val packageName: String? = null,
    ) : UiIntent
    data class Reorder(val intent: ReorderIntent) : UiIntent
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

/** Editing the manual order of the home app list. */
sealed interface ReorderIntent {
    /** Enter reorder mode, starting from the order shown. */
    data object Start : ReorderIntent

    /** Move [packageName] to the position currently held by [target] (drag). */
    data class MoveTo(val packageName: String, val target: String) : ReorderIntent
    data class MoveUp(val packageName: String) : ReorderIntent
    data class MoveDown(val packageName: String) : ReorderIntent

    /** Leave reorder mode keeping the saved order. */
    data object Cancel : ReorderIntent

    /** Leave reorder mode and save the edited order. */
    data object Save : ReorderIntent
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

/**
 * Localised words the view model puts into or checks against its state: the name of the system
 * original space and the word typed to confirm a rebind. The Activity reads them from resources.
 */
internal data class SpaceLabels(
    val baseSpaceName: String,
    val bindingConfirmation: String,
)
