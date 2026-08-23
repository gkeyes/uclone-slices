package com.uclone.slices.v2.ui

import androidx.lifecycle.ViewModel
import com.uclone.slices.v2.BuildConfig
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SeedMode
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

enum class ManagerDestination {
    Spaces,
    AddApp,
    PackageDetails,
    RuntimeStatus,
}

sealed interface OperationUiState {
    data object Idle : OperationUiState
    data object Refreshing : OperationUiState
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
    val confirmation: String = "",
)

data class SlotsUiState(
    val destination: ManagerDestination = ManagerDestination.Spaces,
    val runtimeReady: Boolean = false,
    val runtimeCompatible: Boolean = false,
    val buildId: String = "",
    val initialLoadComplete: Boolean = false,
    val configuredAccountsExpanded: Boolean = false,
    val installedApps: List<InstalledApp> = emptyList(),
    val packages: List<PackageSnapshot> = emptyList(),
    val selected: PackageSnapshot? = null,
    val slotName: String = "",
    val seed: SeedMode = SeedMode.Blank,
    val operation: OperationUiState = OperationUiState.Idle,
    val notice: UiNotice? = null,
    val pendingConfigurationPackage: String? = null,
    val registrationRecovery: RegistrationRecoveryUi? = null,
    val pendingRename: RenameSpaceUi? = null,
    val pendingDelete: DeleteSpaceUi? = null,
    val pendingUnenroll: UnenrollAppUi? = null,
) {
    val busy: Boolean
        get() = operation !is OperationUiState.Idle
    val operationsAllowed: Boolean
        get() = runtimeReady && runtimeCompatible
}

sealed interface UiIntent {
    data object Refresh : UiIntent
    data object OpenAddApps : UiIntent
    data object OpenRuntimeStatus : UiIntent
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
    data class SlotNameChanged(val value: String) : UiIntent
    data class SeedChanged(val value: SeedMode) : UiIntent
    data object CreateSlot : UiIntent
    data class ActivateSlot(val slotId: String) : UiIntent
    data class RequestRenameSlot(val slotId: String) : UiIntent
    data class RenameSlotNameChanged(val value: String) : UiIntent
    data object ConfirmRenameSlot : UiIntent
    data object DismissRenameSlot : UiIntent
    data class RequestDeleteSlot(val slotId: String) : UiIntent
    data object ConfirmDeleteSlot : UiIntent
    data object DismissDeleteSlot : UiIntent
    data class RequestUnenrollApp(val packageName: String) : UiIntent
    data class UnenrollConfirmationChanged(val value: String) : UiIntent
    data object ConfirmUnenrollApp : UiIntent
    data object DismissUnenrollApp : UiIntent
    data object DismissNotice : UiIntent
    data object DismissRegistrationRecovery : UiIntent
    data class RegistrationRecoveryConfirmationChanged(val value: String) : UiIntent
    data object ConfirmRegistrationRecovery : UiIntent
}

internal class SlotsViewModel(
    private val client: RuntimeClient,
    private val installedApps: InstalledAppsSource,
    dispatcher: CoroutineDispatcher,
    private val uiPreferences: ManagerUiPreferences = MemoryManagerUiPreferences(),
) : ViewModel() {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val mutableState = MutableStateFlow(
        SlotsUiState(
            configuredAccountsExpanded = uiPreferences.configuredAccountsExpanded,
        ),
    )
    private val operationLock = Any()
    private var operationActive = false
    val state: StateFlow<SlotsUiState> = mutableState.asStateFlow()

    init {
        runOperation(OperationUiState.Refreshing) { refresh() }
    }

    fun onIntent(intent: UiIntent) {
        when (intent) {
            UiIntent.Refresh -> runOperation(OperationUiState.Refreshing) { refresh() }
            UiIntent.OpenAddApps -> navigateTo(ManagerDestination.AddApp)
            UiIntent.OpenRuntimeStatus -> navigateTo(ManagerDestination.RuntimeStatus)
            is UiIntent.OpenPackage -> openPackage(intent.packageName)
            is UiIntent.SetConfiguredAccountsExpanded -> {
                uiPreferences.configuredAccountsExpanded = intent.expanded
                mutableState.update {
                    it.copy(configuredAccountsExpanded = intent.expanded)
                }
            }
            is UiIntent.SetLaunchAfterReboot ->
                setLaunchAfterReboot(intent.packageName, intent.enabled)
            is UiIntent.QuickActivateSlot ->
                quickActivateSlot(intent.packageName, intent.slotId)
            UiIntent.NavigateBack -> navigateBack()
            is UiIntent.RequestConfigureApp -> {
                if (!state.value.operationsAllowed) {
                    showUnavailableOrMismatch()
                } else if (!state.value.busy) {
                    mutableState.update {
                        it.copy(pendingConfigurationPackage = intent.packageName, notice = null)
                    }
                }
            }
            UiIntent.ConfirmConfigureApp -> confirmConfigureApp()
            UiIntent.DismissConfigureApp -> mutableState.update {
                it.copy(pendingConfigurationPackage = null)
            }
            is UiIntent.SlotNameChanged ->
                mutableState.update { it.copy(slotName = intent.value) }
            is UiIntent.SeedChanged -> mutableState.update { it.copy(seed = intent.value) }
            UiIntent.CreateSlot -> createSlot()
            is UiIntent.ActivateSlot -> activateSlot(intent.slotId)
            is UiIntent.RequestRenameSlot -> requestRenameSlot(intent.slotId)
            is UiIntent.RenameSlotNameChanged -> mutableState.update {
                it.copy(pendingRename = it.pendingRename?.copy(name = intent.value))
            }
            UiIntent.ConfirmRenameSlot -> confirmRenameSlot()
            UiIntent.DismissRenameSlot -> mutableState.update { it.copy(pendingRename = null) }
            is UiIntent.RequestDeleteSlot -> requestDeleteSlot(intent.slotId)
            UiIntent.ConfirmDeleteSlot -> confirmDeleteSlot()
            UiIntent.DismissDeleteSlot -> mutableState.update { it.copy(pendingDelete = null) }
            is UiIntent.RequestUnenrollApp -> requestUnenrollApp(intent.packageName)
            is UiIntent.UnenrollConfirmationChanged -> mutableState.update {
                it.copy(
                    pendingUnenroll = it.pendingUnenroll?.copy(confirmation = intent.value),
                )
            }
            UiIntent.ConfirmUnenrollApp -> confirmUnenrollApp()
            UiIntent.DismissUnenrollApp -> mutableState.update {
                it.copy(pendingUnenroll = null)
            }
            UiIntent.DismissNotice -> mutableState.update { it.copy(notice = null) }
            UiIntent.DismissRegistrationRecovery -> mutableState.update {
                it.copy(registrationRecovery = null)
            }
            is UiIntent.RegistrationRecoveryConfirmationChanged -> mutableState.update {
                it.copy(
                    registrationRecovery = it.registrationRecovery?.copy(
                        confirmation = intent.value,
                    ),
                )
            }
            UiIntent.ConfirmRegistrationRecovery -> confirmRegistrationRecovery()
        }
    }

    override fun onCleared() {
        scope.cancel()
    }

    private fun navigateTo(destination: ManagerDestination) {
        if (state.value.busy) return
        mutableState.update { it.copy(destination = destination, notice = null) }
    }

    private fun navigateBack() {
        if (state.value.busy) return
        mutableState.update { current ->
            when (current.destination) {
                ManagerDestination.Spaces -> current
                ManagerDestination.AddApp,
                ManagerDestination.RuntimeStatus,
                -> current.copy(destination = ManagerDestination.Spaces, notice = null)
                ManagerDestination.PackageDetails -> current.copy(
                    destination = ManagerDestination.Spaces,
                    selected = null,
                    slotName = "",
                    seed = SeedMode.Blank,
                    notice = null,
                    registrationRecovery = null,
                    pendingRename = null,
                    pendingDelete = null,
                )
            }
        }
    }

    private fun openPackage(packageName: String) {
        val snapshot = state.value
        if (!snapshot.runtimeReady) {
            mutableState.update { it.copy(notice = UiNotice.RuntimeUnavailable) }
            return
        }
        val configured = snapshot.packages.firstOrNull { it.packageName == packageName }
        if (configured == null) {
            if (!snapshot.runtimeCompatible) {
                mutableState.update { it.copy(notice = UiNotice.RuntimeVersionMismatch) }
                return
            }
            mutableState.update {
                it.copy(pendingConfigurationPackage = packageName, notice = null)
            }
            return
        }
        if (!snapshot.runtimeCompatible) {
            mutableState.update {
                it.copy(
                    selected = configured,
                    destination = ManagerDestination.PackageDetails,
                    notice = UiNotice.RuntimeVersionMismatch,
                )
            }
            return
        }
        if (configured.bindingState == BindingState.LegacyConfirmationRequired) {
            showLegacyRebind(configured)
            return
        }
        if (configured.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        runOperation(OperationUiState.OpeningPackage(packageName)) {
            applyOpenPackageReply(
                reply = client.execute(RuntimeCommand.GetPackage(packageName)),
                successNotice = null,
            )
        }
    }

    private fun confirmConfigureApp() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val packageName = snapshot.pendingConfigurationPackage ?: return
        val signing = signingFor(packageName) ?: run {
            mutableState.update { it.copy(notice = UiNotice.SigningUnavailable) }
            return
        }
        runOperation(OperationUiState.Configuring(packageName)) {
            applyOpenPackageReply(
                reply = client.execute(RuntimeCommand.Enroll(packageName, signing)),
                successNotice = UiNotice.ConfigurationCompleted,
            )
        }
    }

    private fun createSlot() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val selected = snapshot.selected ?: return
        val packageName = selected.packageName
        if (selected.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val name = snapshot.slotName.trim()
        if (name.isEmpty()) {
            mutableState.update { it.copy(notice = UiNotice.InvalidInput) }
            return
        }
        val seed = snapshot.seed
        runOperation(OperationUiState.CreatingSpace(packageName, name)) {
            applyPackageReply(
                client.execute(RuntimeCommand.CreateSlot(packageName, name, seed)),
                successNotice = UiNotice.SpaceCreated(name),
            )
        }
    }

    private fun activateSlot(slotId: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val selected = snapshot.selected ?: return
        if (selected.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val targetName = selected.slots
            .firstOrNull { it.id == slotId }
            ?.let(::spaceDisplayName)
            ?: slotId
        val switched = selected.activeSlot != slotId
        runOperation(
            OperationUiState.ActivatingSpace(selected.packageName, targetName, switched),
        ) {
            applyPackageReply(
                client.execute(RuntimeCommand.ActivateSlot(selected.packageName, slotId)),
                successNotice = UiNotice.SpaceActivated(targetName, switched),
            )
        }
    }

    private fun quickActivateSlot(packageName: String, slotId: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy) return
        val packageSnapshot = snapshot.packages.firstOrNull {
            it.packageName == packageName
        }
        val target = packageSnapshot?.slots?.firstOrNull { it.id == slotId }
        if (packageSnapshot == null || target == null) {
            mutableState.update { it.copy(notice = UiNotice.MissingEntity) }
            return
        }
        if (packageSnapshot.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val targetName = spaceDisplayName(target)
        val switched = packageSnapshot.activeSlot != slotId
        runOperation(OperationUiState.ActivatingSpace(packageName, targetName, switched)) {
            applyHomePackageReply(
                client.execute(RuntimeCommand.ActivateSlot(packageName, slotId)),
                successNotice = UiNotice.SpaceActivated(targetName, switched),
            )
        }
    }

    private fun setLaunchAfterReboot(packageName: String, enabled: Boolean) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy) return
        val packageSnapshot = snapshot.packages.firstOrNull {
            it.packageName == packageName
        }
        if (packageSnapshot == null) {
            mutableState.update { it.copy(notice = UiNotice.MissingEntity) }
            return
        }
        if (packageSnapshot.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        runOperation(OperationUiState.SavingRebootLaunch(packageName)) {
            applyHomePackageReply(
                client.execute(RuntimeCommand.SetLaunchAfterReboot(packageName, enabled)),
                successNotice = null,
            )
        }
    }

    private fun requestRenameSlot(slotId: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy || slotId == BASE_SLOT_ID) return
        val selected = snapshot.selected ?: return
        if (selected.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val slot = selected.slots.firstOrNull { it.id == slotId } ?: return
        mutableState.update {
            it.copy(
                pendingRename = RenameSpaceUi(
                    packageName = selected.packageName,
                    slotId = slot.id,
                    originalName = slot.name,
                ),
                pendingDelete = null,
                notice = null,
            )
        }
    }

    private fun confirmRenameSlot() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val pending = snapshot.pendingRename ?: return
        if (snapshot.packages.none {
                it.packageName == pending.packageName && it.bindingState == BindingState.Ready
            }
        ) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val name = pending.name.trim()
        if (name.isEmpty() || name == pending.originalName) {
            mutableState.update { it.copy(notice = UiNotice.InvalidInput) }
            return
        }
        val packageName = pending.packageName
        val slotId = pending.slotId
        runOperation(OperationUiState.RenamingSpace(packageName, name)) {
            applyPackageReply(
                client.execute(RuntimeCommand.RenameSlot(packageName, slotId, name)),
                successNotice = UiNotice.SpaceRenamed(name),
            )
        }
    }

    private fun requestDeleteSlot(slotId: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy || slotId == BASE_SLOT_ID) return
        val selected = snapshot.selected ?: return
        if (selected.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        if (slotId == selected.activeSlot) return
        val slot = selected.slots.firstOrNull { it.id == slotId } ?: return
        mutableState.update {
            it.copy(
                pendingDelete = DeleteSpaceUi(
                    packageName = selected.packageName,
                    slotId = slot.id,
                    name = slot.name,
                ),
                pendingRename = null,
                notice = null,
            )
        }
    }

    private fun confirmDeleteSlot() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val pending = snapshot.pendingDelete ?: return
        if (snapshot.packages.none {
                it.packageName == pending.packageName && it.bindingState == BindingState.Ready
            }
        ) {
            mutableState.update { it.copy(notice = UiNotice.IdentityProtected) }
            return
        }
        val packageName = pending.packageName
        val slotId = pending.slotId
        val name = pending.name
        runOperation(OperationUiState.DeletingSpace(packageName, name)) {
            applyPackageReply(
                client.execute(RuntimeCommand.DeleteSlot(packageName, slotId)),
                successNotice = UiNotice.SpaceDeleted(name),
            )
        }
    }

    private fun requestUnenrollApp(packageName: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy) return
        val packageSnapshot = snapshot.packages.firstOrNull {
            it.packageName == packageName
        } ?: return
        val appLabel = snapshot.installedApps
            .firstOrNull { it.packageName == packageName }
            ?.label
            ?: packageName
        mutableState.update {
            it.copy(
                pendingUnenroll = UnenrollAppUi(
                    packageName = packageName,
                    appLabel = appLabel,
                    affectedSpaces = packageSnapshot.slots
                        .filterNot { slot -> slot.id == BASE_SLOT_ID }
                        .map(::spaceDisplayName),
                ),
                pendingConfigurationPackage = null,
                pendingRename = null,
                pendingDelete = null,
                notice = null,
            )
        }
    }

    private fun confirmUnenrollApp() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val pending = snapshot.pendingUnenroll ?: return
        if (pending.confirmation != DELETION_CONFIRMATION) {
            mutableState.update { it.copy(notice = UiNotice.InvalidInput) }
            return
        }
        val packageName = pending.packageName
        val appLabel = pending.appLabel
        runOperation(OperationUiState.UnenrollingApp(packageName, appLabel)) {
            applyUnenrollReply(
                packageName = packageName,
                appLabel = appLabel,
                reply = client.execute(RuntimeCommand.Unenroll(packageName)),
            )
        }
    }

    private fun confirmRegistrationRecovery() {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val recovery = snapshot.registrationRecovery ?: return
        if (recovery.confirmation != BINDING_CONFIRMATION) {
            mutableState.update { it.copy(notice = UiNotice.InvalidInput) }
            return
        }
        val packageName = recovery.packageName
        val signing = signingFor(packageName) ?: run {
            mutableState.update { it.copy(notice = UiNotice.SigningUnavailable) }
            return
        }
        runOperation(OperationUiState.RebindingConfiguration(packageName)) {
            applyPackageReply(
                client.execute(RuntimeCommand.RebindPackage(packageName, signing, true)),
                successNotice = UiNotice.ConfigurationRebound,
            )
        }
    }

    private fun runOperation(
        operationState: OperationUiState,
        operation: suspend () -> Unit,
    ) {
        synchronized(operationLock) {
            if (operationActive) return
            operationActive = true
            mutableState.update {
                it.copy(
                    operation = operationState,
                    notice = null,
                    pendingConfigurationPackage = if (
                        operationState is OperationUiState.Configuring
                    ) {
                        null
                    } else {
                        it.pendingConfigurationPackage
                    },
                    pendingRename = if (
                        operationState is OperationUiState.RenamingSpace
                    ) {
                        null
                    } else {
                        it.pendingRename
                    },
                    pendingDelete = if (
                        operationState is OperationUiState.DeletingSpace
                    ) {
                        null
                    } else {
                        it.pendingDelete
                    },
                    pendingUnenroll = if (
                        operationState is OperationUiState.UnenrollingApp
                    ) {
                        null
                    } else {
                        it.pendingUnenroll
                    },
                )
            }
        }
        scope.launch {
            try {
                operation()
            } finally {
                synchronized(operationLock) {
                    operationActive = false
                    mutableState.update {
                        it.copy(
                            operation = OperationUiState.Idle,
                            initialLoadComplete = it.initialLoadComplete ||
                                operationState is OperationUiState.Refreshing,
                        )
                    }
                }
            }
        }
    }

    private suspend fun refresh() {
        val localApps = runCatching(installedApps::load).getOrElse {
            mutableState.update { state ->
                state.copy(installedApps = emptyList(), notice = UiNotice.LocalAppsUnavailable)
            }
            emptyList()
        }
        mutableState.update { it.copy(installedApps = localApps) }
        when (val probe = client.execute(RuntimeCommand.Probe)) {
            is RuntimeReply.Capabilities -> {
                mutableState.update {
                    it.copy(
                        runtimeReady = true,
                        runtimeCompatible = probe.buildId == BuildConfig.VERSION_NAME,
                        buildId = probe.buildId,
                        notice = if (probe.buildId == BuildConfig.VERSION_NAME) {
                            it.notice
                        } else {
                            UiNotice.RuntimeVersionMismatch
                        },
                    )
                }
                refreshPackages()
            }
            is RuntimeReply.Error -> showError(probe.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private suspend fun refreshPackages() {
        when (val reply = client.execute(RuntimeCommand.ListPackages)) {
            is RuntimeReply.Packages -> {
                val packages = if (state.value.runtimeCompatible) {
                    reconcileBindings(reply.packages)
                } else {
                    reply.packages
                }
                mutableState.update { current ->
                val selectedPackage = current.selected?.packageName
                val selected = packages.firstOrNull {
                    it.packageName == selectedPackage
                }
                current.copy(
                    packages = packages.sortedBy(PackageSnapshot::packageName),
                    selected = selected,
                    seed = if (selected?.activeSlot == BASE_SLOT_ID) {
                        current.seed
                    } else {
                        SeedMode.Blank
                    },
                    destination = if (
                        current.destination == ManagerDestination.PackageDetails &&
                        selected == null
                    ) {
                        ManagerDestination.Spaces
                    } else {
                        current.destination
                    },
                    pendingRename = null,
                    pendingDelete = null,
                    pendingUnenroll = null,
                )
                }
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Package,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private suspend fun reconcileBindings(
        snapshots: List<PackageSnapshot>,
    ): List<PackageSnapshot> = snapshots.map { snapshot ->
        if (
            snapshot.bindingState != BindingState.LegacyUnbound &&
            snapshot.bindingState != BindingState.RebindRequired
        ) {
            return@map snapshot
        }
        val signing = signingFor(snapshot.packageName)
        if (signing == null) {
            mutableState.update { it.copy(notice = UiNotice.SigningUnavailable) }
            return@map snapshot
        }
        when (
            val reply = client.execute(
                RuntimeCommand.RebindPackage(snapshot.packageName, signing, false),
            )
        ) {
            is RuntimeReply.Package -> reply.packageSnapshot
            is RuntimeReply.Error -> {
                showError(reply.code)
                snapshot
            }
            RuntimeReply.TransportFailure -> {
                showTransportFailure()
                snapshot
            }
            else -> {
                showError(ErrorCode.OperationFailed)
                snapshot
            }
        }
    }

    private fun showLegacyRebind(snapshot: PackageSnapshot) {
        mutableState.update {
            it.copy(
                registrationRecovery = RegistrationRecoveryUi(
                    packageName = snapshot.packageName,
                    affectedSpaces = snapshot.slots
                        .filterNot { slot -> slot.id == BASE_SLOT_ID }
                        .map(::spaceDisplayName),
                ),
                pendingConfigurationPackage = null,
                notice = null,
            )
        }
    }

    private fun signingFor(packageName: String) = state.value.installedApps
        .firstOrNull { it.packageName == packageName }
        ?.signingIdentity

    private fun showUnavailableOrMismatch() {
        mutableState.update {
            it.copy(
                notice = if (it.runtimeReady) {
                    UiNotice.RuntimeVersionMismatch
                } else {
                    UiNotice.RuntimeUnavailable
                },
            )
        }
    }

    private fun applyOpenPackageReply(reply: RuntimeReply, successNotice: UiNotice?) {
        applyPackageReply(reply, successNotice)
    }

    private fun applyPackageReply(
        reply: RuntimeReply,
        successNotice: UiNotice?,
    ) {
        when (reply) {
            is RuntimeReply.Package -> mutableState.update { current ->
                val packages = current.packages
                    .filterNot { it.packageName == reply.packageSnapshot.packageName } +
                    reply.packageSnapshot
                current.copy(
                    packages = packages.sortedBy(PackageSnapshot::packageName),
                    selected = reply.packageSnapshot,
                    destination = ManagerDestination.PackageDetails,
                    slotName = "",
                    seed = if (reply.packageSnapshot.activeSlot == BASE_SLOT_ID) {
                        current.seed
                    } else {
                        SeedMode.Blank
                    },
                    notice = successNotice,
                    registrationRecovery = null,
                    pendingRename = null,
                    pendingDelete = null,
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun applyHomePackageReply(
        reply: RuntimeReply,
        successNotice: UiNotice?,
    ) {
        when (reply) {
            is RuntimeReply.Package -> mutableState.update { current ->
                val packageSnapshot = reply.packageSnapshot
                current.copy(
                    packages = (
                        current.packages.filterNot {
                            it.packageName == packageSnapshot.packageName
                        } + packageSnapshot
                    ).sortedBy(PackageSnapshot::packageName),
                    selected = if (
                        current.selected?.packageName == packageSnapshot.packageName
                    ) {
                        packageSnapshot
                    } else {
                        current.selected
                    },
                    notice = successNotice,
                    registrationRecovery = null,
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun applyUnenrollReply(
        packageName: String,
        appLabel: String,
        reply: RuntimeReply,
    ) {
        when (reply) {
            RuntimeReply.Ack -> mutableState.update { current ->
                val removedSelected = current.selected?.packageName == packageName
                current.copy(
                    packages = current.packages.filterNot {
                        it.packageName == packageName
                    },
                    selected = if (removedSelected) null else current.selected,
                    destination = if (
                        removedSelected &&
                        current.destination == ManagerDestination.PackageDetails
                    ) {
                        ManagerDestination.Spaces
                    } else {
                        current.destination
                    },
                    notice = UiNotice.AppUnenrolled(appLabel),
                    registrationRecovery = null,
                    pendingUnenroll = null,
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            is RuntimeReply.Capabilities,
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun showError(code: ErrorCode) {
        val notice = when (code) {
            ErrorCode.InvalidRequest -> UiNotice.InvalidInput
            ErrorCode.NotFound -> UiNotice.MissingEntity
            ErrorCode.StateConflict -> UiNotice.StateChanged
            ErrorCode.IdentityMismatch -> UiNotice.IdentityProtected
            ErrorCode.OperationFailed -> UiNotice.OperationFailed
        }
        mutableState.update { it.copy(notice = notice) }
    }

    private fun showTransportFailure() {
        mutableState.update {
            it.copy(
                runtimeReady = false,
                runtimeCompatible = false,
                notice = UiNotice.RuntimeUnavailable,
                pendingRename = null,
                pendingDelete = null,
                pendingUnenroll = null,
            )
        }
    }
}

internal const val BASE_SLOT_ID = "base"
internal const val DELETION_CONFIRMATION = "删除"
internal const val BINDING_CONFIRMATION = "绑定"
