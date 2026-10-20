package com.uclone.slices.v2.ui

import androidx.lifecycle.ViewModel
import com.uclone.slices.v2.BuildConfig
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.apps.loadAppOptimized
import com.uclone.slices.v2.apps.loadCatalogOptimized
import com.uclone.slices.v2.apps.loadPackagesOptimized
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.NoRuntimeMaintenance
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeMaintenance
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SeedMode
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

internal class SlotsViewModel(
    private val client: RuntimeClient,
    private val installedApps: InstalledAppsSource,
    dispatcher: CoroutineDispatcher,
    private val uiPreferences: ManagerUiPreferences = MemoryManagerUiPreferences(),
    private val maintenance: RuntimeMaintenance = NoRuntimeMaintenance(),
    private val labels: SpaceLabels,
) : ViewModel() {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val mutableState = MutableStateFlow(
        SlotsUiState(
            configuredAccountsExpanded = uiPreferences.configuredAccountsExpanded,
            homeAppOrder = uiPreferences.homeAppOrder,
        ),
    )
    private val navigationEvents = Channel<SlotsNavigation>(Channel.UNLIMITED)

    /**
     * Moves for the nav host, each delivered once. Buffered, so a move requested while no
     * collector is attached (between configuration changes) is made when one attaches again.
     */
    val navigation: Flow<SlotsNavigation> = navigationEvents.receiveAsFlow()

    // The screen on top of the navigation back stack, as last reported by the nav host. Only read
    // to decide what a result means for the visible page; navigation itself is the nav host's.
    @Volatile
    private var destination = ManagerDestination.Spaces

    @Volatile
    private var restoredPackage: String? = null
    private val noticeIds = AtomicLong()
    private val operationLock = Any()
    private var operationActive = false

    // Bumped whenever a refresh replaces installedApps with only the managed apps, so a catalog
    // load that started earlier can never mark that partial list as the full catalog.
    private val catalogGeneration = AtomicInteger()

    @Volatile
    private var runtimeLogJob: Job? = null
    val state: StateFlow<SlotsUiState> = mutableState.asStateFlow()

    init {
        runOperation(OperationUiState.Refreshing) { refresh() }
    }

    fun onIntent(intent: UiIntent) {
        when (intent) {
            UiIntent.Refresh -> runOperation(OperationUiState.Refreshing) { refresh() }
            UiIntent.AppResumed -> {
                // Reconnect an unavailable Runtime; automatic account updates use a passive read
                // so returning never swallows a quick tap behind a busy refresh.
                val snapshot = state.value
                if (snapshot.initialLoadComplete && !snapshot.runtimeReady) {
                    runOperation(OperationUiState.Refreshing) { refresh() }
                } else if (snapshot.operationsAllowed && snapshot.packages.any { it.defaultSlot != null }) {
                    refreshAutomaticAccounts(snapshot.packages)
                }
            }
            UiIntent.RestartRuntime -> runOperation(OperationUiState.RestartingRuntime) {
                runtimeLogJob?.cancel()
                val restarted = maintenance.restartRuntime()
                refresh()
                // A log read issued while restarting describes the old process; drop it.
                runtimeLogJob?.cancel()
                val current = state.value
                mutableState.update {
                    it.copy(
                        posted = when {
                            current.runtimeReady && current.runtimeCompatible ->
                                post(UiNotice.RuntimeRestarted)
                            !restarted || !current.runtimeReady ->
                                post(UiNotice.RuntimeRestartFailed)
                            else -> it.posted
                        },
                        runtimeLog = null,
                    )
                }
            }
            UiIntent.LoadRuntimeLog -> {
                runtimeLogJob?.cancel()
                runtimeLogJob = scope.launch {
                    val log = maintenance.readLogTail()
                    ensureActive()
                    mutableState.update { it.copy(runtimeLog = log.orEmpty()) }
                }
            }
            UiIntent.OpenAddApps -> openAddApps()
            UiIntent.OpenRuntimeStatus -> navigateTo(SlotsNavigation.ToRuntimeStatus)
            is UiIntent.OpenBackupRestore -> navigateTo(
                SlotsNavigation.ToBackupRestore(intent.packageName, intent.slotId),
            )
            is UiIntent.OpenPackage -> openPackage(intent.packageName)
            is UiIntent.SetConfiguredAccountsExpanded -> {
                uiPreferences.configuredAccountsExpanded = intent.expanded
                mutableState.update {
                    it.copy(configuredAccountsExpanded = intent.expanded)
                }
            }
            is UiIntent.SetLaunchAfterReboot ->
                setLaunchAfterReboot(intent.packageName, intent.enabled)
            is UiIntent.SetDefaultSlot -> setDefaultSlot(intent.packageName, intent.slotId)
            is UiIntent.QuickActivateSlot ->
                quickActivateSlot(intent.packageName, intent.slotId)
            is UiIntent.DestinationChanged ->
                destinationChanged(intent.destination, intent.packageName)
            is UiIntent.Reorder -> reorder(intent.intent)
            is UiIntent.RequestConfigureApp -> requestConfigureApp(intent.packageName)
            UiIntent.ConfirmConfigureApp -> confirmConfigureApp()
            UiIntent.DismissConfigureApp -> mutableState.update {
                it.copy(pendingConfigurationPackage = null)
            }
            is UiIntent.SeedChanged -> mutableState.update { it.copy(seed = intent.value) }
            is UiIntent.CreateSlot -> createSlot(intent.name)
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
                    pendingUnenroll = it.pendingUnenroll?.copy(confirmed = intent.confirmed),
                )
            }
            UiIntent.ConfirmUnenrollApp -> confirmUnenrollApp()
            UiIntent.DismissUnenrollApp -> mutableState.update {
                it.copy(pendingUnenroll = null)
            }
            is UiIntent.DismissNotice -> mutableState.update {
                // Only the notice that timed out or was swiped away is cleared, never a newer one
                // that replaced it in the meantime.
                if (it.posted?.id == intent.id) it.copy(posted = null) else it
            }
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

    private fun post(notice: UiNotice) = PostedNotice(noticeIds.incrementAndGet(), notice)

    override fun onCleared() {
        scope.cancel()
    }

    private fun navigate(navigation: SlotsNavigation) {
        navigationEvents.trySend(navigation)
    }

    /** A move the user asked for; dropped while an operation runs, like every other tap. */
    private fun navigateTo(navigation: SlotsNavigation) {
        if (state.value.busy) return
        navigate(navigation)
    }

    private fun destinationChanged(destination: ManagerDestination, packageName: String?) {
        val previous = this.destination
        this.destination = destination
        // A package page restored by the back stack (after process death) names its package
        // while this view model has no selection yet: select it now if it is known, otherwise
        // when the refresh lists it.
        restoredPackage = null
        if (
            destination == ManagerDestination.PackageDetails &&
            packageName != null &&
            state.value.selected == null
        ) {
            restoredPackage = packageName
            mutableState.update { current ->
                if (current.selected != null) return@update current
                current.packages.firstOrNull { it.packageName == packageName }
                    ?.let { current.copy(selected = it) }
                    ?: current
            }
        }
        settleVisiblePage()
        // Back on the spaces list from another page (a recreated host reports the same page
        // again, which changes nothing): the package page is gone, drop what belonged to it.
        if (destination == ManagerDestination.Spaces && previous != ManagerDestination.Spaces) {
            mutableState.update { current ->
                current.copy(
                    selected = null,
                    seed = SeedMode.Blank,
                    registrationRecovery = null,
                    pendingRename = null,
                    pendingDelete = null,
                )
            }
        }
    }

    /**
     * Makes the page on top usable with what is loaded, once no refresh is running: the Add App
     * page gets the full catalog, and a package page without a package (restored by the back
     * stack after process death while the Runtime could not list it) returns home. Called when
     * the nav host reports a page and when a refresh ends, so whichever comes last acts.
     */
    private fun settleVisiblePage() {
        val snapshot = state.value
        if (
            snapshot.operation is OperationUiState.Refreshing ||
            snapshot.operation == OperationUiState.RestartingRuntime
        ) {
            // A refresh drops the catalog when it starts and replaces the app list when it
            // lists the packages; it settles the page when it ends.
            return
        }
        when (destination) {
            ManagerDestination.AddApp -> if (
                !snapshot.appCatalogLoaded && !snapshot.appCatalogLoading
            ) {
                loadAppCatalog()
            }
            ManagerDestination.PackageDetails -> if (snapshot.selected == null) {
                navigate(SlotsNavigation.ToSpaces)
            }
            ManagerDestination.Spaces,
            ManagerDestination.BackupRestore,
            ManagerDestination.RuntimeStatus,
            -> Unit
        }
    }

    private fun reorder(intent: ReorderIntent) {
        when (intent) {
            ReorderIntent.Start -> mutableState.update { current ->
                val keys = current.orderedManagedApps().map(InstalledApp::packageName)
                if (keys.size > 1) current.copy(reorderDraft = keys) else current
            }
            is ReorderIntent.MoveTo -> editDraft {
                HomeOrder.moveTo(it, intent.packageName, intent.target)
            }
            is ReorderIntent.MoveUp -> editDraft { HomeOrder.moveUp(it, intent.packageName) }
            is ReorderIntent.MoveDown -> editDraft { HomeOrder.moveDown(it, intent.packageName) }
            ReorderIntent.Cancel -> mutableState.update { it.copy(reorderDraft = null) }
            ReorderIntent.Save -> {
                val snapshot = state.value
                if (snapshot.reorderDraft == null) return
                val order = snapshot.reorderKeys()
                uiPreferences.homeAppOrder = order
                mutableState.update { it.copy(homeAppOrder = order, reorderDraft = null) }
            }
        }
    }

    private fun editDraft(edit: (List<String>) -> List<String>) {
        mutableState.update { current ->
            if (current.reorderDraft == null) {
                current
            } else {
                current.copy(reorderDraft = edit(current.reorderKeys()))
            }
        }
    }

    private fun openAddApps() {
        // The catalog loads when the nav host reports the page (destinationChanged), which also
        // covers a page the back stack restores.
        navigateTo(SlotsNavigation.ToAddApps)
    }

    private fun loadAppCatalog() {
        val generation = catalogGeneration.get()
        mutableState.update { it.copy(appCatalogLoading = true) }
        scope.launch {
            val result = runCatching { installedApps.loadCatalogOptimized() }
            if (catalogGeneration.get() != generation) return@launch
            if (result.isFailure) {
                mutableState.update {
                    if (catalogGeneration.get() != generation) return@update it
                    it.copy(
                        appCatalogLoading = false,
                        appCatalogLoaded = false,
                        posted = post(UiNotice.LocalAppsUnavailable),
                    )
                }
                return@launch
            }
            val catalog = result.getOrThrow()
            mutableState.update { current ->
                if (catalogGeneration.get() != generation) return@update current
                val managedNames = current.packages
                    .asSequence()
                    .map(PackageSnapshot::packageName)
                    .toSet()
                val preserved = current.installedApps
                    .filter { it.packageName in managedNames }
                    .associateBy(InstalledApp::packageName)
                val catalogNames = catalog.asSequence().map(InstalledApp::packageName).toSet()
                val merged = (
                    catalog.map { preserved[it.packageName] ?: it } +
                        preserved.values.filter { it.packageName !in catalogNames }
                    ).sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
                current.copy(
                    installedApps = merged,
                    appCatalogLoading = false,
                    appCatalogLoaded = true,
                )
            }
        }
    }

    private fun requestConfigureApp(packageName: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy) return
        if (signingFor(packageName) != null) {
            mutableState.update {
                it.copy(pendingConfigurationPackage = packageName)
            }
            return
        }
        runOperation(OperationUiState.PreparingConfiguration(packageName)) {
            val result = runCatching { installedApps.loadAppOptimized(packageName) }
            if (result.isFailure) {
                mutableState.update { it.copy(posted = post(UiNotice.LocalAppsUnavailable)) }
                return@runOperation
            }
            val app = result.getOrNull()
            if (app?.signingIdentity == null) {
                mutableState.update { it.copy(posted = post(UiNotice.SigningUnavailable)) }
                return@runOperation
            }
            mutableState.update { current ->
                current.copy(
                    installedApps = (
                        current.installedApps.filterNot { it.packageName == packageName } + app
                        ).sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label }),
                    pendingConfigurationPackage = packageName,
                )
            }
        }
    }

    private fun openPackage(packageName: String) {
        val snapshot = state.value
        if (!snapshot.runtimeReady) {
            mutableState.update { it.copy(posted = post(UiNotice.RuntimeUnavailable)) }
            return
        }
        val configured = snapshot.packages.firstOrNull { it.packageName == packageName }
        if (configured == null) {
            if (!snapshot.runtimeCompatible) {
                mutableState.update { it.copy(posted = post(UiNotice.RuntimeVersionMismatch)) }
                return
            }
            mutableState.update {
                it.copy(pendingConfigurationPackage = packageName)
            }
            return
        }
        if (!snapshot.runtimeCompatible) {
            mutableState.update {
                it.copy(
                    selected = configured,
                    posted = post(UiNotice.RuntimeVersionMismatch),
                )
            }
            navigate(SlotsNavigation.ToPackage(packageName))
            return
        }
        if (configured.bindingState == BindingState.LegacyConfirmationRequired) {
            showLegacyRebind(configured)
            return
        }
        if (configured.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        if (snapshot.busy) return
        // Open the page from the home snapshot right away; the Runtime read that follows only
        // refreshes it in place.
        mutableState.update {
            it.copy(
                selected = configured,
                seed = SeedMode.Blank,
            )
        }
        navigate(SlotsNavigation.ToPackage(packageName))
        runOperation(OperationUiState.OpeningPackage(packageName)) {
            applyPackageReply(
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
            mutableState.update { it.copy(posted = post(UiNotice.SigningUnavailable)) }
            return
        }
        runOperation(OperationUiState.Configuring(packageName)) {
            applyPackageReply(
                reply = client.execute(RuntimeCommand.Enroll(packageName, signing)),
                successNotice = UiNotice.ConfigurationCompleted,
            )
        }
    }

    private fun createSlot(rawName: String) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        val selected = snapshot.selected ?: return
        val packageName = selected.packageName
        if (selected.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        val name = rawName.trim()
        if (name.isEmpty()) {
            mutableState.update { it.copy(posted = post(UiNotice.InvalidInput)) }
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
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        val targetName = selected.slots
            .firstOrNull { it.id == slotId }
            ?.let { spaceDisplayName(it, labels.baseSpaceName) }
            ?: slotId
        val switched = selected.activeSlot != slotId
        runOperation(
            OperationUiState.ActivatingSpace(selected.packageName, targetName, switched, slotId),
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
            mutableState.update { it.copy(posted = post(UiNotice.MissingEntity)) }
            return
        }
        if (packageSnapshot.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        val targetName = spaceDisplayName(target, labels.baseSpaceName)
        val switched = packageSnapshot.activeSlot != slotId
        runOperation(
            OperationUiState.ActivatingSpace(packageName, targetName, switched, slotId),
        ) {
            applyHomePackageReply(
                client.execute(RuntimeCommand.ActivateSlot(packageName, slotId)),
                successNotice = UiNotice.SpaceActivated(targetName, switched),
            )
        }
    }

    private fun refreshAutomaticAccounts(before: List<PackageSnapshot>) {
        // This read does not make the UI busy. A tap can proceed immediately, and a stale
        // refresh never overwrites the result of a newer account operation.
        scope.launch {
            val reply = client.execute(RuntimeCommand.ListPackages)
            if (reply is RuntimeReply.Packages) {
                synchronized(operationLock) {
                    if (!operationActive && state.value.packages === before) {
                        mutableState.update { current ->
                            val selected = current.selected?.let { selected ->
                                reply.packages.firstOrNull { it.packageName == selected.packageName }
                            }
                            current.copy(
                                packages = reply.packages,
                                selected = selected,
                                seed = if (selected != null && selected.activeSlot != BASE_SLOT_ID) {
                                    SeedMode.Blank
                                } else {
                                    current.seed
                                },
                            )
                        }
                    }
                }
            }
        }
    }

    private fun setDefaultSlot(packageName: String, slotId: String?) {
        val snapshot = state.value
        if (!snapshot.operationsAllowed) {
            showUnavailableOrMismatch()
            return
        }
        if (snapshot.busy) return
        val target = snapshot.packages.firstOrNull { it.packageName == packageName }
        if (target == null || (slotId != null && target.slots.none { it.id == slotId })) {
            mutableState.update { it.copy(posted = post(UiNotice.MissingEntity)) }
            return
        }
        if (target.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        if (target.defaultSlot == slotId) return
        runOperation(OperationUiState.SavingDefaultSlot(packageName)) {
            applyHomePackageReply(
                client.execute(RuntimeCommand.SetDefaultSlot(packageName, slotId)),
                successNotice = null,
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
            mutableState.update { it.copy(posted = post(UiNotice.MissingEntity)) }
            return
        }
        if (packageSnapshot.bindingState != BindingState.Ready) {
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
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
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
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
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
            return
        }
        val name = pending.name.trim()
        if (name.isEmpty() || name == pending.originalName) {
            mutableState.update { it.copy(posted = post(UiNotice.InvalidInput)) }
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
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
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
                    disablesDefaultAccount = selected.defaultSlot == slot.id,
                ),
                pendingRename = null,
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
            mutableState.update { it.copy(posted = post(UiNotice.IdentityProtected)) }
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
                        .map { slot -> spaceDisplayName(slot, labels.baseSpaceName) },
                ),
                pendingConfigurationPackage = null,
                pendingRename = null,
                pendingDelete = null,
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
        if (!pending.confirmed) {
            mutableState.update { it.copy(posted = post(UiNotice.InvalidInput)) }
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
        if (recovery.confirmation != labels.bindingConfirmation) {
            mutableState.update { it.copy(posted = post(UiNotice.InvalidInput)) }
            return
        }
        val packageName = recovery.packageName
        val signing = signingFor(packageName) ?: run {
            mutableState.update { it.copy(posted = post(UiNotice.SigningUnavailable)) }
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
                    posted = null,
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
                if (
                    operationState is OperationUiState.Refreshing ||
                    operationState == OperationUiState.RestartingRuntime
                ) {
                    settleVisiblePage()
                }
            }
        }
    }

    private suspend fun refresh() {
        catalogGeneration.incrementAndGet()
        mutableState.update {
            it.copy(
                appCatalogLoading = false,
                appCatalogLoaded = false,
            )
        }
        when (val probe = client.execute(RuntimeCommand.Probe)) {
            is RuntimeReply.Capabilities -> {
                val compatible = probe.buildId == BuildConfig.RUNTIME_PROTOCOL_BUILD_ID &&
                    probe.binaryBuildId == BuildConfig.RUNTIME_PROTOCOL_BUILD_ID
                mutableState.update {
                    it.copy(
                        runtimeReady = true,
                        runtimeCompatible = compatible,
                        buildId = probe.binaryBuildId.orEmpty(),
                        packages = if (compatible) it.packages else emptyList(),
                        selected = if (compatible) it.selected else null,
                        posted = if (compatible) {
                            it.posted
                        } else {
                            post(UiNotice.RuntimeVersionMismatch)
                        },
                    )
                }
                if (compatible) {
                    refreshPackages()
                } else if (destination != ManagerDestination.Spaces) {
                    navigate(SlotsNavigation.ToSpaces)
                }
            }
            is RuntimeReply.Error -> showError(probe.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.AccountIoLeaseReply,
            is RuntimeReply.AccountIoStatuses,
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            is RuntimeReply.RestoreBatch,
            is RuntimeReply.RestoreItem,
            -> showError(ErrorCode.OperationFailed)
        }
        // The catalog this refresh dropped is reloaded by settleVisiblePage() once the operation
        // has ended, if the Add App page is showing then.
    }

    private suspend fun refreshPackages() {
        when (val reply = client.execute(RuntimeCommand.ListPackages)) {
            is RuntimeReply.Packages -> {
                val packageNames = reply.packages
                    .asSequence()
                    .map(PackageSnapshot::packageName)
                    .toSet()
                val localResult = runCatching {
                    installedApps.loadPackagesOptimized(packageNames)
                }
                val loadedApps = localResult.getOrDefault(emptyList())
                val loadedByPackage = loadedApps.associateBy(InstalledApp::packageName)
                val visibleApps = packageNames
                    .map { packageName ->
                        loadedByPackage[packageName]
                            ?: InstalledApp(packageName = packageName, label = packageName)
                    }
                    .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
                mutableState.update { current ->
                    current.copy(
                        installedApps = visibleApps,
                        posted = if (localResult.isFailure) {
                            post(UiNotice.LocalAppsUnavailable)
                        } else {
                            current.posted
                        },
                    )
                }

                val packages = if (state.value.runtimeCompatible) {
                    reconcileBindings(reply.packages)
                } else {
                    reply.packages
                }
                var packagePageGone = false
                mutableState.update { current ->
                    val selectedPackage = current.selected?.packageName
                        ?: restoredPackage.takeIf {
                            destination == ManagerDestination.PackageDetails
                        }
                    val selected = packages.firstOrNull {
                        it.packageName == selectedPackage
                    }
                    packagePageGone = destination == ManagerDestination.PackageDetails &&
                        selected == null
                    current.copy(
                        packages = packages.sortedBy(PackageSnapshot::packageName),
                        selected = selected,
                        seed = if (selected?.activeSlot == BASE_SLOT_ID) {
                            current.seed
                        } else {
                            SeedMode.Blank
                        },
                        pendingRename = null,
                        pendingDelete = null,
                        pendingUnenroll = null,
                    )
                }
                if (packagePageGone) navigate(SlotsNavigation.ToSpaces)
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.AccountIoLeaseReply,
            is RuntimeReply.AccountIoStatuses,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Package,
            is RuntimeReply.RestoreBatch,
            is RuntimeReply.RestoreItem,
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
            mutableState.update { it.copy(posted = post(UiNotice.SigningUnavailable)) }
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
                        .map { slot -> spaceDisplayName(slot, labels.baseSpaceName) },
                ),
                pendingConfigurationPackage = null,
            )
        }
    }

    private fun signingFor(packageName: String) = state.value.installedApps
        .firstOrNull { it.packageName == packageName }
        ?.signingIdentity

    private fun showUnavailableOrMismatch() {
        mutableState.update {
            it.copy(
                posted = post(
                    if (it.runtimeReady) {
                        UiNotice.RuntimeVersionMismatch
                    } else {
                        UiNotice.RuntimeUnavailable
                    },
                ),
            )
        }
    }

    private fun applyPackageReply(
        reply: RuntimeReply,
        successNotice: UiNotice?,
    ) {
        when (reply) {
            is RuntimeReply.Package -> {
                mutableState.update { current ->
                    val packages = current.packages
                        .filterNot { it.packageName == reply.packageSnapshot.packageName } +
                        reply.packageSnapshot
                    current.copy(
                        packages = packages.sortedBy(PackageSnapshot::packageName),
                        selected = reply.packageSnapshot,
                        seed = if (reply.packageSnapshot.activeSlot == BASE_SLOT_ID) {
                            current.seed
                        } else {
                            SeedMode.Blank
                        },
                        posted = successNotice?.let(::post),
                        registrationRecovery = null,
                        pendingRename = null,
                        pendingDelete = null,
                    )
                }
                if (destination != ManagerDestination.PackageDetails) {
                    navigate(SlotsNavigation.ToPackage(reply.packageSnapshot.packageName))
                }
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.AccountIoLeaseReply,
            is RuntimeReply.AccountIoStatuses,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            is RuntimeReply.RestoreBatch,
            is RuntimeReply.RestoreItem,
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
                    posted = successNotice?.let(::post),
                    registrationRecovery = null,
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            RuntimeReply.Ack,
            is RuntimeReply.AccountIoLeaseReply,
            is RuntimeReply.AccountIoStatuses,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            is RuntimeReply.RestoreBatch,
            is RuntimeReply.RestoreItem,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun applyUnenrollReply(
        packageName: String,
        appLabel: String,
        reply: RuntimeReply,
    ) {
        when (reply) {
            RuntimeReply.Ack -> {
                var removedSelected = false
                mutableState.update { current ->
                    removedSelected = current.selected?.packageName == packageName
                    current.copy(
                        packages = current.packages.filterNot {
                            it.packageName == packageName
                        },
                        selected = if (removedSelected) null else current.selected,
                        posted = post(UiNotice.AppUnenrolled(appLabel)),
                        registrationRecovery = null,
                        pendingUnenroll = null,
                    )
                }
                if (removedSelected && destination == ManagerDestination.PackageDetails) {
                    navigate(SlotsNavigation.ToSpaces)
                }
            }
            is RuntimeReply.Error -> showError(reply.code)
            RuntimeReply.TransportFailure -> showTransportFailure()
            is RuntimeReply.Capabilities,
            is RuntimeReply.AccountIoLeaseReply,
            is RuntimeReply.AccountIoStatuses,
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            is RuntimeReply.RestoreBatch,
            is RuntimeReply.RestoreItem,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun showError(code: ErrorCode) {
        mutableState.update { it.copy(posted = post(code.toUiNotice())) }
    }

    private fun showTransportFailure() {
        mutableState.update {
            it.copy(
                runtimeReady = false,
                runtimeCompatible = false,
                posted = post(UiNotice.RuntimeUnavailable),
                pendingRename = null,
                pendingDelete = null,
                pendingUnenroll = null,
            )
        }
    }
}
