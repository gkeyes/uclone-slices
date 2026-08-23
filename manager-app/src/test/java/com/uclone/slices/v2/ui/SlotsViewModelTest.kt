package com.uclone.slices.v2.ui

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
import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class SlotsViewModelTest {
    private val appSource = InstalledAppsSource {
        listOf(InstalledApp("com.example.app", "Example", signingIdentity = testSigning()))
    }

    @Test
    fun initializationRefreshesRuntimePackagesAndLocalApps() {
        val client = FakeRuntimeClient()

        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        assertEquals(
            listOf(RuntimeCommand.Probe, RuntimeCommand.ListPackages),
            client.commands,
        )
        assertEquals("com.example.app", viewModel.state.value.installedApps.single().packageName)
        assertTrue(viewModel.state.value.runtimeReady)
        assertTrue(viewModel.state.value.initialLoadComplete)
        assertIs<OperationUiState.Idle>(viewModel.state.value.operation)
    }

    @Test
    fun unconfiguredAppRequiresConfirmationBeforeEnrollment() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        val commandsBeforeRequest = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))

        assertEquals("com.example.app", viewModel.state.value.pendingConfigurationPackage)
        assertEquals(commandsBeforeRequest, client.commands)

        viewModel.onIntent(UiIntent.ConfirmConfigureApp)

        assertEquals(RuntimeCommand.Enroll("com.example.app", testSigning()), client.commands.last())
        assertEquals(ManagerDestination.PackageDetails, viewModel.state.value.destination)
        assertEquals(UiNotice.ConfigurationCompleted, viewModel.state.value.notice)
        assertNull(viewModel.state.value.pendingConfigurationPackage)
    }

    @Test
    fun openingConfiguredAppUsesGetPackage() {
        val snapshot = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))

        assertEquals(RuntimeCommand.GetPackage(snapshot.packageName), client.commands.last())
        assertEquals(ManagerDestination.PackageDetails, viewModel.state.value.destination)
    }

    @Test
    fun legacyUnboundConfigurationIsAutomaticallyBoundWithoutChangingAccounts() {
        val snapshot = packageSnapshot(bindingState = BindingState.LegacyUnbound)
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        assertEquals(
            RuntimeCommand.RebindPackage(snapshot.packageName, testSigning(), false),
            client.commands.last(),
        )
        assertEquals(BindingState.Ready, viewModel.state.value.packages.single().bindingState)
        assertEquals(snapshot.slots, viewModel.state.value.packages.single().slots)
    }

    @Test
    fun legacyChangedConfigurationOffersLosslessRebindWithKnownSpaces() {
        val snapshot = packageSnapshot(
            active = "slot-1",
            bindingState = BindingState.LegacyConfirmationRequired,
        )
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))

        assertEquals(
            RegistrationRecoveryUi(snapshot.packageName, listOf("Work")),
            viewModel.state.value.registrationRecovery,
        )
        assertTrue(client.commands.none { it is RuntimeCommand.RebindPackage })
    }

    @Test
    fun signerMismatchProtectsTheSnapshotWithoutOfferingARepairButton() {
        val protected = packageSnapshot(bindingState = BindingState.RebindRequired)
        val client = FakeRuntimeClient(
            initialPackages = listOf(protected),
            rebindError = ErrorCode.IdentityMismatch,
        )

        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        assertEquals(UiNotice.IdentityProtected, viewModel.state.value.notice)
        assertEquals(BindingState.RebindRequired, viewModel.state.value.packages.single().bindingState)
        assertNull(viewModel.state.value.registrationRecovery)
        val commands = client.commands.toList()
        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))
        assertEquals(commands, client.commands)
        assertEquals(UiNotice.IdentityProtected, viewModel.state.value.notice)
    }

    @Test
    fun missingSigningInformationNeverStartsEnrollmentOrLegacyTakeover() {
        val unsignedSource = InstalledAppsSource {
            listOf(InstalledApp("com.example.app", "Example"))
        }
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, unsignedSource, Dispatchers.Unconfined)
        val commands = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))
        viewModel.onIntent(UiIntent.ConfirmConfigureApp)

        assertEquals(commands, client.commands)
        assertEquals(UiNotice.SigningUnavailable, viewModel.state.value.notice)
    }

    @Test
    fun dismissingRegistrationRecoveryDoesNotCallRuntime() {
        val legacy = packageSnapshot(bindingState = BindingState.LegacyConfirmationRequired)
        val client = FakeRuntimeClient(initialPackages = listOf(legacy))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        val commandsBeforeDismiss = client.commands.toList()

        viewModel.onIntent(UiIntent.DismissRegistrationRecovery)

        assertNull(viewModel.state.value.registrationRecovery)
        assertEquals(commandsBeforeDismiss, client.commands)
    }

    @Test
    fun legacyRebindRequiresExactConfirmationAndPreservesTheActiveAccount() {
        val legacy = packageSnapshot(
            active = "slot-1",
            bindingState = BindingState.LegacyConfirmationRequired,
        )
        val client = FakeRuntimeClient(initialPackages = listOf(legacy))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        val commandsBeforeConfirm = client.commands.toList()

        viewModel.onIntent(UiIntent.ConfirmRegistrationRecovery)

        assertEquals(commandsBeforeConfirm, client.commands)
        assertEquals(UiNotice.InvalidInput, viewModel.state.value.notice)

        viewModel.onIntent(UiIntent.RegistrationRecoveryConfirmationChanged("绑定"))
        viewModel.onIntent(UiIntent.ConfirmRegistrationRecovery)

        assertEquals(
            RuntimeCommand.RebindPackage("com.example.app", testSigning(), true),
            client.commands.last(),
        )
        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertEquals(legacy.slots, viewModel.state.value.selected?.slots)
        assertNull(viewModel.state.value.registrationRecovery)
        assertEquals(UiNotice.ConfigurationRebound, viewModel.state.value.notice)
    }

    @Test
    fun navigationReturnsToSpacesWithoutCallingRuntime() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        val commandsBeforeNavigation = client.commands.toList()

        viewModel.onIntent(UiIntent.OpenAddApps)
        assertEquals(ManagerDestination.AddApp, viewModel.state.value.destination)
        viewModel.onIntent(UiIntent.NavigateBack)
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)

        viewModel.onIntent(UiIntent.OpenRuntimeStatus)
        assertEquals(ManagerDestination.RuntimeStatus, viewModel.state.value.destination)
        viewModel.onIntent(UiIntent.NavigateBack)

        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
        assertEquals(commandsBeforeNavigation, client.commands)
    }

    @Test
    fun operationArgumentsAreFrozenAtIntentTime() = runBlocking {
        val client = FreezingRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        viewModel.onIntent(UiIntent.SlotNameChanged("Work"))
        viewModel.onIntent(UiIntent.SeedChanged(SeedMode.CloneBase))

        viewModel.onIntent(UiIntent.CreateSlot)
        client.createStarted.await()
        viewModel.onIntent(UiIntent.SlotNameChanged("Changed later"))
        viewModel.onIntent(UiIntent.SeedChanged(SeedMode.Blank))
        client.releaseCreate.complete(Unit)

        assertTrue(
            client.commands.contains(
                RuntimeCommand.CreateSlot(
                    packageName = "com.example.app",
                    name = "Work",
                    seed = SeedMode.CloneBase,
                ),
            ),
        )
    }

    @Test
    fun activeOrdinarySpaceCanBeRenamedWithoutChangingItsIdentity() {
        val initial = packageSnapshot(active = "slot-1")
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))

        viewModel.onIntent(UiIntent.RequestRenameSlot("slot-1"))
        viewModel.onIntent(UiIntent.RenameSlotNameChanged("Personal"))
        viewModel.onIntent(UiIntent.ConfirmRenameSlot)

        assertEquals(
            RuntimeCommand.RenameSlot("com.example.app", "slot-1", "Personal"),
            client.commands.last(),
        )
        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertEquals(
            "Personal",
            viewModel.state.value.selected?.slots?.first { it.id == "slot-1" }?.name,
        )
        assertEquals(UiNotice.SpaceRenamed("Personal"), viewModel.state.value.notice)
        assertNull(viewModel.state.value.pendingRename)
    }

    @Test
    fun renameRejectsBaseBlankAndUnchangedNamesBeforeRuntime() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))
        val commandsBeforeRename = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestRenameSlot("base"))
        assertNull(viewModel.state.value.pendingRename)

        viewModel.onIntent(UiIntent.RequestRenameSlot("slot-1"))
        viewModel.onIntent(UiIntent.ConfirmRenameSlot)
        assertEquals(commandsBeforeRename, client.commands)
        assertEquals(UiNotice.InvalidInput, viewModel.state.value.notice)

        viewModel.onIntent(UiIntent.RenameSlotNameChanged("   "))
        viewModel.onIntent(UiIntent.ConfirmRenameSlot)
        assertEquals(commandsBeforeRename, client.commands)
    }

    @Test
    fun inactiveOrdinarySpaceCanBeDeletedAfterConfirmation() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))

        viewModel.onIntent(UiIntent.RequestDeleteSlot("slot-1"))
        assertEquals(
            DeleteSpaceUi("com.example.app", "slot-1", "Work"),
            viewModel.state.value.pendingDelete,
        )
        viewModel.onIntent(UiIntent.ConfirmDeleteSlot)

        assertEquals(
            RuntimeCommand.DeleteSlot("com.example.app", "slot-1"),
            client.commands.last(),
        )
        assertEquals(listOf("base"), viewModel.state.value.selected?.slots?.map { it.id })
        assertEquals(UiNotice.SpaceDeleted("Work"), viewModel.state.value.notice)
        assertNull(viewModel.state.value.pendingDelete)
    }

    @Test
    fun baseAndActiveSpaceNeverOpenTheDeleteConfirmation() {
        val initial = packageSnapshot(active = "slot-1")
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))
        val commandsBeforeDelete = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestDeleteSlot("base"))
        assertNull(viewModel.state.value.pendingDelete)
        viewModel.onIntent(UiIntent.RequestDeleteSlot("slot-1"))

        assertNull(viewModel.state.value.pendingDelete)
        assertEquals(commandsBeforeDelete, client.commands)
    }

    @Test
    fun deleteFailureKeepsTheCurrentSnapshotWithoutOfferingRegistrationReset() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))
        viewModel.onIntent(UiIntent.RequestDeleteSlot("slot-1"))
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.ConfirmDeleteSlot)

        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
        assertNull(viewModel.state.value.registrationRecovery)
        assertEquals(initial.slots, viewModel.state.value.selected?.slots)
        assertTrue(viewModel.state.value.runtimeReady)
    }

    @Test
    fun blankSlotNameNeverReachesRuntime() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        viewModel.onIntent(UiIntent.SlotNameChanged("   "))
        val commandsBeforeCreate = client.commands.toList()

        viewModel.onIntent(UiIntent.CreateSlot)

        assertEquals(commandsBeforeCreate, client.commands)
        assertEquals(UiNotice.InvalidInput, viewModel.state.value.notice)
    }

    @Test
    fun busyDropsASecondOperationInsteadOfQueuingIt() = runBlocking {
        val client = BlockingRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))
        viewModel.onIntent(UiIntent.ConfirmConfigureApp)
        client.firstStarted.await()
        assertTrue(viewModel.state.value.busy)
        assertEquals(
            OperationUiState.Configuring("com.example.app"),
            viewModel.state.value.operation,
        )
        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.second"))

        assertEquals(1, client.openCommands.size)
        client.releaseFirst.complete(Unit)
        assertFalse(viewModel.state.value.busy)
        assertEquals(
            listOf<RuntimeCommand>(RuntimeCommand.Enroll("com.example.app", testSigning())),
            client.openCommands,
        )
    }

    @Test
    fun refreshedPackagesSynchronizeSelectedByPackageName() {
        val first = packageSnapshot(active = "base")
        val client = FakeRuntimeClient(initialPackages = listOf(first))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(first.packageName))
        client.snapshot = first.copy(activeSlot = "slot-1")

        viewModel.onIntent(UiIntent.Refresh)

        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertEquals(ManagerDestination.PackageDetails, viewModel.state.value.destination)
    }

    @Test
    fun refreshClearsCloneBaseSeedWhenTheActiveSpaceChangedOutsideManager() {
        val first = packageSnapshot(active = "base")
        val client = FakeRuntimeClient(initialPackages = listOf(first))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(first.packageName))
        viewModel.onIntent(UiIntent.SeedChanged(SeedMode.CloneBase))
        client.snapshot = first.copy(activeSlot = "slot-1")

        viewModel.onIntent(UiIntent.Refresh)

        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertEquals(SeedMode.Blank, viewModel.state.value.seed)
    }

    @Test
    fun fiveWireErrorsRemainDistinctOutsideTheRebindPath() {
        val expected = mapOf(
            ErrorCode.InvalidRequest to UiNotice.InvalidInput,
            ErrorCode.NotFound to UiNotice.MissingEntity,
            ErrorCode.StateConflict to UiNotice.StateChanged,
            ErrorCode.IdentityMismatch to UiNotice.IdentityProtected,
            ErrorCode.OperationFailed to UiNotice.OperationFailed,
        )
        expected.forEach { (code, notice) ->
            val client = FakeRuntimeClient()
            val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
            configureApp(viewModel)
            viewModel.onIntent(UiIntent.SlotNameChanged("Work"))
            client.nextReply = RuntimeReply.Error(code)

            viewModel.onIntent(UiIntent.CreateSlot)

            assertEquals(notice, viewModel.state.value.notice)
            assertNull(viewModel.state.value.registrationRecovery)
            assertTrue(viewModel.state.value.runtimeReady)
        }
    }

    @Test
    fun busyDropsDuplicateRebindConfirmationAndKeepsFrozenPackage() = runBlocking {
        val client = BlockingRebindRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        viewModel.onIntent(UiIntent.RegistrationRecoveryConfirmationChanged("绑定"))

        viewModel.onIntent(UiIntent.ConfirmRegistrationRecovery)
        client.rebindStarted.await()
        viewModel.onIntent(UiIntent.ConfirmRegistrationRecovery)

        assertTrue(viewModel.state.value.busy)
        assertEquals(
            listOf(RuntimeCommand.RebindPackage("com.example.app", testSigning(), true)),
            client.rebindCommands,
        )
        client.releaseRebind.complete(Unit)
        assertFalse(viewModel.state.value.busy)
        assertEquals("com.example.app", viewModel.state.value.selected?.packageName)
    }

    @Test
    fun mismatchedRuntimeSendsOnlyProbeAndClearsRuntimeState() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(
            initialPackages = listOf(initial),
            buildId = "0.1.6",
        )
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        assertTrue(viewModel.state.value.runtimeReady)
        assertFalse(viewModel.state.value.runtimeCompatible)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertTrue(viewModel.state.value.packages.isEmpty())
        assertNull(viewModel.state.value.selected)

        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))

        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
        assertNull(viewModel.state.value.selected)
        viewModel.onIntent(UiIntent.ActivateSlot("slot-1"))
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
    }

    @Test
    fun transportFailureClearsRuntimeReadyAndPreservesTypedNotice() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        viewModel.onIntent(UiIntent.SlotNameChanged("Work"))
        client.nextReply = RuntimeReply.TransportFailure

        viewModel.onIntent(UiIntent.CreateSlot)

        assertEquals(UiNotice.RuntimeUnavailable, viewModel.state.value.notice)
        assertFalse(viewModel.state.value.runtimeReady)
    }

    @Test
    fun runtimeDependentIntentDoesNotExecuteWhileDisconnected() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        val commandsBeforeOpen = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))

        assertEquals(commandsBeforeOpen, client.commands)
        assertEquals(UiNotice.RuntimeUnavailable, viewModel.state.value.notice)
        assertNull(viewModel.state.value.pendingConfigurationPackage)
    }

    @Test
    fun configuredAccountExpansionPersistsUntilTheUserChangesIt() {
        val preferences = MemoryManagerUiPreferences()
        val first = SlotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            preferences,
        )

        first.onIntent(UiIntent.SetConfiguredAccountsExpanded(true))
        first.onIntent(UiIntent.Refresh)
        val reopened = SlotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            preferences,
        )

        assertTrue(first.state.value.configuredAccountsExpanded)
        assertTrue(reopened.state.value.configuredAccountsExpanded)
        reopened.onIntent(UiIntent.SetConfiguredAccountsExpanded(false))
        val reopenedAgain = SlotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            preferences,
        )
        assertFalse(reopenedAgain.state.value.configuredAccountsExpanded)
    }

    @Test
    fun quickActivationUpdatesOnlyTheHomeSnapshotAndLaunchesCurrentSpaceToo() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))

        assertEquals(
            RuntimeCommand.ActivateSlot("com.example.app", "slot-1"),
            client.commands.last(),
        )
        assertEquals("slot-1", viewModel.state.value.packages.single().activeSlot)
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
        assertNull(viewModel.state.value.selected)
        assertEquals(
            UiNotice.SpaceActivated("Work", switched = true),
            viewModel.state.value.notice,
        )

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))

        assertEquals(
            UiNotice.SpaceActivated("Work", switched = false),
            viewModel.state.value.notice,
        )
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
    }

    @Test
    fun quickActivationFailureDoesNotOptimisticallyChangeTheHighlight() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))

        assertEquals("base", viewModel.state.value.packages.single().activeSlot)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
    }

    @Test
    fun rebootLaunchSettingUsesTheRuntimeSnapshotWithoutLaunchingTheApp() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.SetLaunchAfterReboot("com.example.app", true))

        assertEquals(
            RuntimeCommand.SetLaunchAfterReboot("com.example.app", true),
            client.commands.last(),
        )
        assertTrue(viewModel.state.value.packages.single().launchAfterReboot)
        assertTrue(client.commands.none { it is RuntimeCommand.ActivateSlot })
        assertNull(viewModel.state.value.notice)
    }

    @Test
    fun rebootLaunchSettingFailureKeepsThePreviousSwitchValue() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.SetLaunchAfterReboot("com.example.app", true))

        assertFalse(viewModel.state.value.packages.single().launchAfterReboot)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
    }

    @Test
    fun quickActivationFreezesArgumentsAndDropsASecondTapWhileBusy() = runBlocking {
        val client = BlockingQuickRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))
        client.activationStarted.await()
        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "base"))

        assertEquals(
            listOf(RuntimeCommand.ActivateSlot("com.example.app", "slot-1")),
            client.activationCommands,
        )
        client.releaseActivation.complete(Unit)
        assertFalse(viewModel.state.value.busy)
        assertEquals("slot-1", viewModel.state.value.packages.single().activeSlot)
        assertEquals(ManagerDestination.Spaces, viewModel.state.value.destination)
    }

    @Test
    fun unenrollRequiresFrozenConfirmationAndMovesTheAppBackToAvailable() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.RequestUnenrollApp("com.example.app"))

        assertEquals(
            UnenrollAppUi(
                packageName = "com.example.app",
                appLabel = "Example",
                affectedSpaces = listOf("Work"),
            ),
            viewModel.state.value.pendingUnenroll,
        )
        val commandsBeforeConfirmation = client.commands.toList()
        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)
        assertEquals(commandsBeforeConfirmation, client.commands)
        assertEquals(UiNotice.InvalidInput, viewModel.state.value.notice)

        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged("删除"))
        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)

        assertEquals(
            RuntimeCommand.Unenroll("com.example.app"),
            client.commands.last(),
        )
        assertTrue(viewModel.state.value.packages.isEmpty())
        assertNull(viewModel.state.value.selected)
        assertNull(viewModel.state.value.pendingUnenroll)
        assertEquals(
            UiNotice.AppUnenrolled("Example"),
            viewModel.state.value.notice,
        )
    }

    @Test
    fun unenrollFailureKeepsTheConfiguredAppAndClearsRuntimeOnTransportFailure() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.RequestUnenrollApp("com.example.app"))
        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged("删除"))
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)

        assertEquals(listOf(initial), viewModel.state.value.packages)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
        assertTrue(viewModel.state.value.runtimeReady)

        viewModel.onIntent(UiIntent.RequestUnenrollApp("com.example.app"))
        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged("删除"))
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)

        assertEquals(listOf(initial), viewModel.state.value.packages)
        assertFalse(viewModel.state.value.runtimeReady)
        assertEquals(UiNotice.RuntimeUnavailable, viewModel.state.value.notice)
    }

    private fun configureApp(viewModel: SlotsViewModel) {
        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))
        viewModel.onIntent(UiIntent.ConfirmConfigureApp)
    }
}

private fun packageSnapshot(
    active: String = "base",
    bindingState: BindingState = BindingState.Ready,
) = PackageSnapshot(
    packageName = "com.example.app",
    activeSlot = active,
    slots = listOf(
        SlotSnapshot("base", "Base"),
        SlotSnapshot("slot-1", "Work"),
    ),
    bindingState = bindingState,
)

private fun testSigning() = SigningIdentity(
    kind = SigningKind.Lineage,
    sha256 = listOf("a".repeat(64)),
)

private val TEST_RUNTIME_VERSION = BuildConfig.VERSION_NAME

private open class FakeRuntimeClient(
    initialPackages: List<PackageSnapshot> = emptyList(),
    private val buildId: String = TEST_RUNTIME_VERSION,
    private val rebindError: ErrorCode? = null,
) : RuntimeClient {
    val commands = mutableListOf<RuntimeCommand>()
    var snapshot = initialPackages.firstOrNull() ?: packageSnapshot()
    var enrolled = initialPackages.isNotEmpty()
    var nextReply: RuntimeReply? = null

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        commands += command
        nextReply?.let {
            nextReply = null
            return it
        }
        return when (command) {
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(buildId)
            RuntimeCommand.ListPackages -> RuntimeReply.Packages(
                if (enrolled) listOf(snapshot) else emptyList(),
            )
            is RuntimeCommand.GetPackage -> RuntimeReply.Package(snapshot)
            is RuntimeCommand.Enroll -> {
                enrolled = true
                snapshot = snapshot.copy(packageName = command.packageName)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.RebindPackage -> {
                rebindError?.let { return RuntimeReply.Error(it) }
                snapshot = snapshot.copy(bindingState = BindingState.Ready)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.CreateSlot -> {
                snapshot = snapshot.copy(
                    slots = snapshot.slots + SlotSnapshot("slot-2", command.name),
                )
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.ActivateSlot -> {
                snapshot = snapshot.copy(activeSlot = command.slotId)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.RenameSlot -> {
                snapshot = snapshot.copy(
                    slots = snapshot.slots.map { slot ->
                        if (slot.id == command.slotId) {
                            slot.copy(name = command.name)
                        } else {
                            slot
                        }
                    },
                )
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.DeleteSlot -> {
                snapshot = snapshot.copy(
                    slots = snapshot.slots.filterNot { it.id == command.slotId },
                )
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.Unenroll -> {
                enrolled = false
                RuntimeReply.Ack
            }
            is RuntimeCommand.SetLaunchAfterReboot -> {
                snapshot = snapshot.copy(launchAfterReboot = command.enabled)
                RuntimeReply.Package(snapshot)
            }
        }
    }
}

private class FreezingRuntimeClient : FakeRuntimeClient() {
    val createStarted = CompletableDeferred<Unit>()
    val releaseCreate = CompletableDeferred<Unit>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        if (command is RuntimeCommand.CreateSlot) {
            createStarted.complete(Unit)
            releaseCreate.await()
        }
        return super.execute(command)
    }
}

private class BlockingRuntimeClient : RuntimeClient {
    val openCommands = mutableListOf<RuntimeCommand>()
    val firstStarted = CompletableDeferred<Unit>()
    val releaseFirst = CompletableDeferred<Unit>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        when (command) {
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION)
            RuntimeCommand.ListPackages -> RuntimeReply.Packages(emptyList())
            is RuntimeCommand.Enroll,
            is RuntimeCommand.GetPackage,
            -> {
                openCommands += command
                firstStarted.complete(Unit)
                releaseFirst.await()
                val packageName = when (command) {
                    is RuntimeCommand.Enroll -> command.packageName
                    is RuntimeCommand.GetPackage -> command.packageName
                    else -> error("unreachable")
                }
                RuntimeReply.Package(
                    PackageSnapshot(
                        packageName = packageName,
                        activeSlot = "base",
                        slots = listOf(SlotSnapshot("base", "Base")),
                    ),
                )
            }
            is RuntimeCommand.CreateSlot,
            is RuntimeCommand.ActivateSlot,
            is RuntimeCommand.RenameSlot,
            is RuntimeCommand.DeleteSlot,
            is RuntimeCommand.RebindPackage,
            is RuntimeCommand.Unenroll,
            is RuntimeCommand.SetLaunchAfterReboot,
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
}

private class BlockingRebindRuntimeClient : RuntimeClient {
    val rebindCommands = mutableListOf<RuntimeCommand.RebindPackage>()
    val rebindStarted = CompletableDeferred<Unit>()
    val releaseRebind = CompletableDeferred<Unit>()
    private val snapshot = packageSnapshot(
        bindingState = BindingState.LegacyConfirmationRequired,
    )

    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        when (command) {
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION)
            RuntimeCommand.ListPackages -> RuntimeReply.Packages(listOf(snapshot))
            is RuntimeCommand.Enroll,
            is RuntimeCommand.GetPackage,
            -> RuntimeReply.Error(ErrorCode.StateConflict)
            is RuntimeCommand.RebindPackage -> {
                rebindCommands += command
                rebindStarted.complete(Unit)
                releaseRebind.await()
                RuntimeReply.Package(snapshot.copy(bindingState = BindingState.Ready))
            }
            is RuntimeCommand.CreateSlot,
            is RuntimeCommand.ActivateSlot,
            is RuntimeCommand.RenameSlot,
            is RuntimeCommand.DeleteSlot,
            is RuntimeCommand.Unenroll,
            is RuntimeCommand.SetLaunchAfterReboot,
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
}

private class BlockingQuickRuntimeClient : RuntimeClient {
    val activationCommands = mutableListOf<RuntimeCommand.ActivateSlot>()
    val activationStarted = CompletableDeferred<Unit>()
    val releaseActivation = CompletableDeferred<Unit>()
    private var snapshot = packageSnapshot()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        when (command) {
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION)
            RuntimeCommand.ListPackages -> RuntimeReply.Packages(listOf(snapshot))
            is RuntimeCommand.ActivateSlot -> {
                activationCommands += command
                activationStarted.complete(Unit)
                releaseActivation.await()
                snapshot = snapshot.copy(activeSlot = command.slotId)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.GetPackage,
            is RuntimeCommand.Enroll,
            is RuntimeCommand.RebindPackage,
            is RuntimeCommand.Unenroll,
            is RuntimeCommand.CreateSlot,
            is RuntimeCommand.RenameSlot,
            is RuntimeCommand.DeleteSlot,
            is RuntimeCommand.SetLaunchAfterReboot,
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
}
