package com.uclone.slices.v2.ui

import com.uclone.slices.v2.BuildConfig
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.apps.OptimizedInstalledAppsSource
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.NoRuntimeMaintenance
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeMaintenance
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SeedMode
import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

class SlotsViewModelTest {
    private val appSource = InstalledAppsSource {
        listOf(InstalledApp("com.example.app", "Example", signingIdentity = testSigning()))
    }

    @Test
    fun unreadableListKeepsThePreviouslyDisplayedAccountsAndSelection() {
        val registered = packageSnapshot()
        val client = object : FakeRuntimeClient(initialPackages = listOf(registered)) {
            var brokenList = false
            override suspend fun execute(command: RuntimeCommand): RuntimeReply =
                if (brokenList && command == RuntimeCommand.ListPackages) {
                    RuntimeReply.Error(ErrorCode.StateConflict)
                } else {
                    super.execute(command)
                }
        }
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage(registered.packageName))
        client.brokenList = true
        viewModel.onIntent(UiIntent.Refresh)
        assertEquals(listOf(registered), viewModel.state.value.packages)
        assertEquals(registered, viewModel.state.value.selected)
        assertEquals(UiNotice.StateChanged, viewModel.state.value.notice)
    }

    @Test
    fun unavailableAccountsRemainVisibleAfterColdStartAndBlockMutations() {
        val registered = packageSnapshot().copy(runtimeReady = false, bindingState = BindingState.LegacyUnbound)
        val client = FakeRuntimeClient(initialPackages = listOf(registered))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        assertEquals(listOf(registered), viewModel.state.value.packages)
        assertTrue(client.commands.none { it is RuntimeCommand.RebindPackage })
        viewModel.onIntent(UiIntent.OpenPackage(registered.packageName))
        assertEquals(registered, viewModel.state.value.selected)
        val before = client.commands.toList()
        listOf(
            UiIntent.ActivateSlot("slot-1"),
            UiIntent.QuickActivateSlot(registered.packageName, "slot-1"),
            UiIntent.CreateSlot("New"),
            UiIntent.SetDefaultSlot(registered.packageName, "base"),
            UiIntent.SetLaunchAfterReboot(registered.packageName, true),
            UiIntent.RequestDeleteSlot("slot-1"),
            UiIntent.RequestRenameSlot("slot-1"),
            UiIntent.RequestUnenrollApp(registered.packageName),
            UiIntent.OpenBackupRestore(registered.packageName),
        ).forEach { intent ->
            viewModel.onIntent(intent)
            assertEquals(before, client.commands, intent.toString())
            assertEquals(UiNotice.PackageUnavailable, viewModel.state.value.notice)
        }
        client.snapshot = registered.copy(runtimeReady = true, bindingState = BindingState.Ready)
        viewModel.onIntent(UiIntent.Refresh)
        assertTrue(viewModel.state.value.packages.single().runtimeReady)
        viewModel.onIntent(UiIntent.QuickActivateSlot(registered.packageName, "slot-1"))
        assertTrue(client.commands.last() is RuntimeCommand.ActivateSlot)
    }

    @Test
    fun pendingModuleMetadataCannotMakeAnOldDaemonAcceptDefaultAccountWrites() {
        val commands = mutableListOf<RuntimeCommand>()
        val client = object : RuntimeClient {
            override suspend fun execute(command: RuntimeCommand): RuntimeReply {
                commands += command
                return if (command == RuntimeCommand.Probe) {
                    // The old executable echoes the newly installed module's environment.
                    com.uclone.slices.v2.runtime.RuntimeProtocol.decode(
                        """{"ok":{"build_id":"${BuildConfig.RUNTIME_PROTOCOL_BUILD_ID}"}}""",
                    )
                } else {
                    RuntimeReply.Error(ErrorCode.InvalidRequest)
                }
            }
        }
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.SetDefaultSlot("com.example.app", "base"))

        assertTrue(viewModel.state.value.runtimeReady)
        assertFalse(viewModel.state.value.runtimeCompatible)
        assertEquals("", viewModel.state.value.buildId)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), commands)
    }

    @Test
    fun matchingMetadataWithADifferentExecutableBlocksWrites() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.Capabilities(TEST_RUNTIME_VERSION, "older-executable")

        viewModel.onIntent(UiIntent.Refresh)
        viewModel.onIntent(UiIntent.SetDefaultSlot("com.example.app", "slot-1"))

        assertFalse(viewModel.state.value.operationsAllowed)
        assertEquals("older-executable", viewModel.state.value.buildId)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
        assertTrue(client.commands.none { it is RuntimeCommand.SetDefaultSlot })
    }

    @Test
    fun defaultAccountChoiceSavesPolicyWithoutActivatingAndCanBeDisabled() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        viewModel.onIntent(UiIntent.SetDefaultSlot("com.example.app", "slot-1"))
        assertEquals(RuntimeCommand.SetDefaultSlot("com.example.app", "slot-1"), client.commands.last())
        assertEquals("base", viewModel.state.value.packages.single().activeSlot)
        assertEquals("slot-1", viewModel.state.value.selected?.defaultSlot)
        viewModel.onIntent(UiIntent.SetDefaultSlot("com.example.app", null))
        assertNull(viewModel.state.value.selected?.defaultSlot)
        assertTrue(client.commands.none { it is RuntimeCommand.ActivateSlot })
    }

    @Test
    fun deletingDefaultAccountWarnsAndClearsPolicyOnlyAfterSuccess() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot().copy(defaultSlot = "slot-1")))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        viewModel.onIntent(UiIntent.RequestDeleteSlot("slot-1"))
        assertTrue(viewModel.state.value.pendingDelete!!.disablesDefaultAccount)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)
        viewModel.onIntent(UiIntent.ConfirmDeleteSlot)
        assertEquals("slot-1", viewModel.state.value.selected?.defaultSlot)
        assertNull(viewModel.state.value.pendingDelete)
        viewModel.onIntent(UiIntent.RequestDeleteSlot("slot-1"))
        assertTrue(viewModel.state.value.pendingDelete!!.disablesDefaultAccount)
        viewModel.onIntent(UiIntent.ConfirmDeleteSlot)
        assertNull(viewModel.state.value.selected?.defaultSlot)
    }

    @Test
    fun resumeRefreshesAutomaticallyChangedAccountsWithoutAUserOperation() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot().copy(defaultSlot = "base")))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.snapshot = client.snapshot.copy(activeSlot = "slot-1")
        viewModel.onIntent(UiIntent.AppResumed)
        assertEquals(RuntimeCommand.ListPackages, client.commands.last())
        assertEquals("slot-1", viewModel.state.value.packages.single().activeSlot)
        assertFalse(viewModel.state.value.busy)
    }

    @Test
    fun aLateAutomaticAccountRefreshNeverOverwritesANewerTap() {
        val original = packageSnapshot().copy(defaultSlot = "base")
        val pending = CompletableDeferred<RuntimeReply>()
        var deferList = false
        val client = object : FakeRuntimeClient(initialPackages = listOf(original)) {
            override suspend fun execute(command: RuntimeCommand): RuntimeReply =
                if (deferList && command == RuntimeCommand.ListPackages) {
                    pending.await()
                } else {
                    super.execute(command)
                }
        }
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        deferList = true
        viewModel.onIntent(UiIntent.AppResumed)
        assertFalse(viewModel.state.value.busy)
        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))
        pending.complete(RuntimeReply.Packages(listOf(original)))
        assertEquals("slot-1", viewModel.state.value.packages.single().activeSlot)
    }

    @Test
    fun initializationRefreshesRuntimePackagesAndLocalApps() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))

        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        val commandsBeforeRequest = client.commands.toList()

        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))

        assertEquals("com.example.app", viewModel.state.value.pendingConfigurationPackage)
        assertEquals(commandsBeforeRequest, client.commands)

        viewModel.onIntent(UiIntent.ConfirmConfigureApp)

        assertEquals(RuntimeCommand.Enroll("com.example.app", testSigning()), client.commands.last())
        assertEquals(ManagerDestination.PackageDetails, host.top)
        assertEquals(UiNotice.ConfigurationCompleted, viewModel.state.value.notice)
        assertNull(viewModel.state.value.pendingConfigurationPackage)
    }

    @Test
    fun openingConfiguredAppUsesGetPackage() {
        val snapshot = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)

        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))

        assertEquals(RuntimeCommand.GetPackage(snapshot.packageName), client.commands.last())
        assertEquals(ManagerDestination.PackageDetails, host.top)
    }

    @Test
    fun legacyUnboundConfigurationIsAutomaticallyBoundWithoutChangingAccounts() {
        val snapshot = packageSnapshot(bindingState = BindingState.LegacyUnbound)
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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

        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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
        val viewModel = slotsViewModel(client, unsignedSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        val commandsBeforeNavigation = client.commands.toList()

        viewModel.onIntent(UiIntent.OpenAddApps)
        assertEquals(ManagerDestination.AddApp, host.top)
        host.back()
        assertEquals(ManagerDestination.Spaces, host.top)

        viewModel.onIntent(UiIntent.OpenRuntimeStatus)
        assertEquals(ManagerDestination.RuntimeStatus, host.top)
        host.back()

        assertEquals(ManagerDestination.Spaces, host.top)
        assertEquals(commandsBeforeNavigation, client.commands)
    }

    @Test
    fun operationArgumentsAreFrozenAtIntentTime() = runBlocking {
        val client = FreezingRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        viewModel.onIntent(UiIntent.SeedChanged(SeedMode.CloneBase))

        viewModel.onIntent(UiIntent.CreateSlot("工作账号"))
        client.createStarted.await()
        viewModel.onIntent(UiIntent.SeedChanged(SeedMode.Blank))
        client.releaseCreate.complete(Unit)

        assertTrue(
            client.commands.contains(
                RuntimeCommand.CreateSlot(
                    packageName = "com.example.app",
                    name = "工作账号",
                    seed = SeedMode.CloneBase,
                ),
            ),
        )
    }

    @Test
    fun activeOrdinarySpaceCanBeRenamedWithoutChangingItsIdentity() {
        val initial = packageSnapshot(active = "slot-1")
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        val commandsBeforeCreate = client.commands.toList()

        viewModel.onIntent(UiIntent.CreateSlot("   "))

        assertEquals(commandsBeforeCreate, client.commands)
        assertEquals(UiNotice.InvalidInput, viewModel.state.value.notice)
    }

    @Test
    fun busyDropsASecondOperationInsteadOfQueuingIt() = runBlocking {
        val client = BlockingRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        viewModel.onIntent(UiIntent.OpenPackage(first.packageName))
        client.snapshot = first.copy(activeSlot = "slot-1")

        viewModel.onIntent(UiIntent.Refresh)

        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertEquals(ManagerDestination.PackageDetails, host.top)
    }

    @Test
    fun refreshClearsCloneBaseSeedWhenTheActiveSpaceChangedOutsideManager() {
        val first = packageSnapshot(active = "base")
        val client = FakeRuntimeClient(initialPackages = listOf(first))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
            val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
            configureApp(viewModel)
            client.nextReply = RuntimeReply.Error(code)

            viewModel.onIntent(UiIntent.CreateSlot("Work"))

            assertEquals(notice, viewModel.state.value.notice)
            assertNull(viewModel.state.value.registrationRecovery)
            assertTrue(viewModel.state.value.runtimeReady)
        }
    }

    @Test
    fun busyDropsDuplicateRebindConfirmationAndKeepsFrozenPackage() = runBlocking {
        val client = BlockingRebindRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)

        assertTrue(viewModel.state.value.runtimeReady)
        assertFalse(viewModel.state.value.runtimeCompatible)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertTrue(viewModel.state.value.packages.isEmpty())
        assertNull(viewModel.state.value.selected)

        viewModel.onIntent(UiIntent.OpenPackage(initial.packageName))

        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertEquals(ManagerDestination.Spaces, host.top)
        assertNull(viewModel.state.value.selected)
        viewModel.onIntent(UiIntent.ActivateSlot("slot-1"))
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), client.commands)
        assertEquals(UiNotice.RuntimeVersionMismatch, viewModel.state.value.notice)
    }

    @Test
    fun matchingVersionsCannotEnableWritesWithoutBothLaunchComponents() {
        for ((system, process) in listOf(false to false, true to false, false to true)) {
            val client = GatedProbeRuntimeClient(RuntimeReply.Capabilities(
                TEST_RUNTIME_VERSION, TEST_RUNTIME_VERSION,
                launchGateReady = system, processViewReady = process,
            ))
            client.gate.complete(Unit)
            val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
            assertTrue(viewModel.state.value.runtimeCompatible)
            assertFalse(viewModel.state.value.operationsAllowed)
            val before = client.commands.toList()
            viewModel.onIntent(UiIntent.CreateSlot("Work"))
            assertEquals(before, client.commands)
        }
    }

    @Test
    fun transportFailureClearsRuntimeReadyAndPreservesTypedNotice() {
        val client = FakeRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        configureApp(viewModel)
        client.nextReply = RuntimeReply.TransportFailure

        viewModel.onIntent(UiIntent.CreateSlot("Work"))

        assertEquals(UiNotice.RuntimeUnavailable, viewModel.state.value.notice)
        assertFalse(viewModel.state.value.runtimeReady)
    }

    @Test
    fun repeatedFailurePostsTheNoticeAgain() {
        val client = FakeRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        val first = assertNotNull(viewModel.state.value.posted)

        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        val second = assertNotNull(viewModel.state.value.posted)

        assertEquals(UiNotice.RuntimeUnavailable, first.notice)
        assertEquals(UiNotice.RuntimeUnavailable, second.notice)
        assertNotEquals(first.id, second.id)
    }

    @Test
    fun expiredNoticeDoesNotDismissTheOneThatReplacedIt() {
        val client = FakeRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        val first = assertNotNull(viewModel.state.value.posted)
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        val second = assertNotNull(viewModel.state.value.posted)

        viewModel.onIntent(UiIntent.DismissNotice(first.id))
        assertEquals(second, viewModel.state.value.posted)

        viewModel.onIntent(UiIntent.DismissNotice(second.id))
        assertNull(viewModel.state.value.posted)
    }

    @Test
    fun navigationKeepsTheVisibleNotice() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)
        viewModel.onIntent(UiIntent.SetLaunchAfterReboot("com.example.app", true))
        val posted = assertNotNull(viewModel.state.value.posted)

        viewModel.onIntent(UiIntent.OpenRuntimeStatus)
        assertEquals(ManagerDestination.RuntimeStatus, host.top)
        assertEquals(posted, viewModel.state.value.posted)

        host.back()
        assertEquals(ManagerDestination.Spaces, host.top)
        assertEquals(posted, viewModel.state.value.posted)
    }

    @Test
    fun runtimeDependentIntentDoesNotExecuteWhileDisconnected() {
        val client = FakeRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
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
        val first = slotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            preferences,
        )

        first.onIntent(UiIntent.SetConfiguredAccountsExpanded(true))
        first.onIntent(UiIntent.Refresh)
        val reopened = slotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            preferences,
        )

        assertTrue(first.state.value.configuredAccountsExpanded)
        assertTrue(reopened.state.value.configuredAccountsExpanded)
        reopened.onIntent(UiIntent.SetConfiguredAccountsExpanded(false))
        val reopenedAgain = slotsViewModel(
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))

        assertEquals(
            RuntimeCommand.ActivateSlot("com.example.app", "slot-1"),
            client.commands.last(),
        )
        assertEquals("slot-1", viewModel.state.value.packages.single().activeSlot)
        assertEquals(ManagerDestination.Spaces, host.top)
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
        assertEquals(ManagerDestination.Spaces, host.top)
    }

    @Test
    fun quickActivationFailureDoesNotOptimisticallyChangeTheHighlight() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))

        assertEquals("base", viewModel.state.value.packages.single().activeSlot)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
        assertEquals(ManagerDestination.Spaces, host.top)
    }

    @Test
    fun rebootLaunchSettingUsesTheRuntimeSnapshotWithoutLaunchingTheApp() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.SetLaunchAfterReboot("com.example.app", true))

        assertFalse(viewModel.state.value.packages.single().launchAfterReboot)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
    }

    @Test
    fun quickActivationFreezesArgumentsAndDropsASecondTapWhileBusy() = runBlocking {
        val client = BlockingQuickRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)

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
        assertEquals(ManagerDestination.Spaces, host.top)
    }

    @Test
    fun unenrollRequiresConfirmationAndMovesTheAppBackToAvailable() {
        val initial = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(initial))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

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

        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged(true))
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
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.RequestUnenrollApp("com.example.app"))
        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged(true))
        client.nextReply = RuntimeReply.Error(ErrorCode.OperationFailed)

        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)

        assertEquals(listOf(initial), viewModel.state.value.packages)
        assertEquals(UiNotice.OperationFailed, viewModel.state.value.notice)
        assertTrue(viewModel.state.value.runtimeReady)

        viewModel.onIntent(UiIntent.RequestUnenrollApp("com.example.app"))
        viewModel.onIntent(UiIntent.UnenrollConfirmationChanged(true))
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.ConfirmUnenrollApp)

        assertEquals(listOf(initial), viewModel.state.value.packages)
        assertFalse(viewModel.state.value.runtimeReady)
        assertEquals(UiNotice.RuntimeUnavailable, viewModel.state.value.notice)
    }

    @Test
    fun busyAndStorageErrorsExplainThemselves() {
        mapOf(
            ErrorCode.IoBusy to UiNotice.AccountIoBusy,
            ErrorCode.InsufficientStorage to UiNotice.StorageInsufficient,
        ).forEach { (code, notice) ->
            val client = FakeRuntimeClient()
            val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
            configureApp(viewModel)
            client.nextReply = RuntimeReply.Error(code)

            viewModel.onIntent(UiIntent.CreateSlot("Work"))

            assertEquals(notice, viewModel.state.value.notice)
            assertTrue(viewModel.state.value.runtimeReady)
        }
    }

    @Test
    fun openingAConfiguredAppShowsItsPageBeforeTheRuntimeReadReturns() = runBlocking<Unit> {
        val client = BlockingGetPackageRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)

        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        client.getStarted.await()

        assertEquals(ManagerDestination.PackageDetails, host.top)
        assertEquals("com.example.app", viewModel.state.value.selected?.packageName)
        assertFalse(viewModel.state.value.operation.blocksScreen)
        client.releaseGet.complete(Unit)
        assertEquals("slot-1", viewModel.state.value.selected?.activeSlot)
        assertFalse(viewModel.state.value.busy)
    }

    @Test
    fun quickActivationShowsProgressOnTheTappedSpaceInsteadOfADialog() = runBlocking<Unit> {
        val client = BlockingQuickRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))
        client.activationStarted.await()

        val operation = assertIs<OperationUiState.ActivatingSpace>(viewModel.state.value.operation)
        assertEquals("slot-1", operation.slotId)
        assertFalse(operation.blocksScreen)
        client.releaseActivation.complete(Unit)
    }

    @Test
    fun configuredAccountsStartExpanded() {
        val viewModel = slotsViewModel(FakeRuntimeClient(), appSource, Dispatchers.Unconfined)

        assertTrue(viewModel.state.value.configuredAccountsExpanded)
    }

    @Test
    fun restartingTheRuntimeReconnectsAndReportsTheOutcome() {
        val client = FakeRuntimeClient()
        val maintenance = FakeRuntimeMaintenance(restarts = true)
        val viewModel = slotsViewModel(
            client,
            appSource,
            Dispatchers.Unconfined,
            MemoryManagerUiPreferences(),
            maintenance,
        )
        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        assertFalse(viewModel.state.value.runtimeReady)

        viewModel.onIntent(UiIntent.RestartRuntime)

        assertEquals(1, maintenance.restartCalls)
        assertTrue(viewModel.state.value.runtimeReady)
        assertEquals(UiNotice.RuntimeRestarted, viewModel.state.value.notice)

        val failingClient = FakeRuntimeClient()
        val failing = slotsViewModel(
            failingClient,
            appSource,
            Dispatchers.Unconfined,
            MemoryManagerUiPreferences(),
            FakeRuntimeMaintenance(restarts = false),
        )
        failingClient.nextReply = RuntimeReply.TransportFailure
        failing.onIntent(UiIntent.RestartRuntime)
        assertEquals(UiNotice.RuntimeRestartFailed, failing.state.value.notice)
    }

    @Test
    fun returningToTheManagerOnlyReconnectsWhenTheRuntimeWasUnavailable() {
        val client = FakeRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val commandsWhileHealthy = client.commands.toList()

        viewModel.onIntent(UiIntent.AppResumed)
        assertEquals(commandsWhileHealthy, client.commands)

        client.nextReply = RuntimeReply.TransportFailure
        viewModel.onIntent(UiIntent.Refresh)
        assertFalse(viewModel.state.value.runtimeReady)
        viewModel.onIntent(UiIntent.AppResumed)

        assertTrue(viewModel.state.value.runtimeReady)
        assertEquals(RuntimeCommand.ListPackages, client.commands.last())
    }

    @Test
    fun runtimeLogIsLoadedOnRequest() {
        val viewModel = slotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            MemoryManagerUiPreferences(),
            FakeRuntimeMaintenance(restarts = true, log = "ucloned start"),
        )

        viewModel.onIntent(UiIntent.LoadRuntimeLog)

        assertEquals("ucloned start", viewModel.state.value.runtimeLog)
    }

    @Test
    fun catalogLoadedDuringARefreshIsNotOverwrittenByTheManagedAppList() {
        val client = HoldingListRuntimeClient()
        val apps = RefreshingCatalogSource()
        val viewModel = slotsViewModel(client, apps, Dispatchers.Unconfined)
        // The nav host reports the Add App page as soon as it is asked to show it.
        FakeNavHost(viewModel)
        // The first catalog read starts a refresh that then waits on ListPackages; whichever side
        // lands first, the Add App page must end with the full catalog, not the managed list.
        apps.duringFirstCatalog = {
            client.holdList = CompletableDeferred()
            viewModel.onIntent(UiIntent.Refresh)
        }

        viewModel.onIntent(UiIntent.OpenAddApps)
        client.holdList?.complete(Unit)

        val state = viewModel.state.value
        assertTrue(state.appCatalogLoaded)
        assertFalse(state.appCatalogLoading)
        assertEquals(2, apps.catalogReads)
        assertEquals(
            setOf("com.example.app", "com.other.app"),
            state.installedApps.map(InstalledApp::packageName).toSet(),
        )
    }

    @Test
    fun restartingTheRuntimeDropsALogReadOfTheOldProcess() {
        val maintenance = FakeRuntimeMaintenance(restarts = true, log = "old process")
        val viewModel = slotsViewModel(
            FakeRuntimeClient(),
            appSource,
            Dispatchers.Unconfined,
            MemoryManagerUiPreferences(),
            maintenance,
        )
        val logRead = CompletableDeferred<Unit>()
        maintenance.beforeLog = { logRead.await() }

        viewModel.onIntent(UiIntent.LoadRuntimeLog)
        viewModel.onIntent(UiIntent.RestartRuntime)
        logRead.complete(Unit)

        assertNull(viewModel.state.value.runtimeLog)
    }

    @Test
    fun leavingThePackagePageDropsItsSelectionAndSheets() {
        val snapshot = packageSnapshot(active = "slot-1")
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))
        viewModel.onIntent(UiIntent.RequestRenameSlot("slot-1"))
        assertNotNull(viewModel.state.value.pendingRename)
        val commandsBeforeBack = client.commands.toList()

        host.back()

        assertEquals(ManagerDestination.Spaces, host.top)
        assertNull(viewModel.state.value.selected)
        assertNull(viewModel.state.value.pendingRename)
        assertEquals(commandsBeforeBack, client.commands)
    }

    @Test
    fun backupRestoreOpensAboveThePageItWasStartedFrom() {
        val snapshot = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))

        viewModel.onIntent(UiIntent.OpenBackupRestore(snapshot.packageName, "slot-1"))

        assertEquals(
            SlotsNavigation.ToBackupRestore(snapshot.packageName, "slot-1"),
            host.moves.last(),
        )
        assertEquals(ManagerDestination.BackupRestore, host.top)
        // Returning keeps the package page and its selection.
        host.back()
        assertEquals(ManagerDestination.PackageDetails, host.top)
        assertEquals(snapshot.packageName, viewModel.state.value.selected?.packageName)
    }

    @Test
    fun navigationRequestsAreDroppedWhileAnOperationRuns() = runBlocking<Unit> {
        val client = BlockingQuickRuntimeClient()
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        viewModel.onIntent(UiIntent.QuickActivateSlot("com.example.app", "slot-1"))
        client.activationStarted.await()
        val movesBefore = host.moves.toList()

        viewModel.onIntent(UiIntent.OpenAddApps)
        viewModel.onIntent(UiIntent.OpenRuntimeStatus)
        viewModel.onIntent(UiIntent.OpenBackupRestore())

        assertEquals(movesBefore, host.moves)
        assertEquals(ManagerDestination.Spaces, host.top)
        client.releaseActivation.complete(Unit)
    }

    @Test
    fun aPackageThatDisappearsOnRefreshTakesItsPageAway() {
        val snapshot = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val host = FakeNavHost(viewModel)
        viewModel.onIntent(UiIntent.OpenPackage(snapshot.packageName))
        client.enrolled = false

        viewModel.onIntent(UiIntent.Refresh)

        assertEquals(SlotsNavigation.ToSpaces, host.moves.last())
        assertEquals(ManagerDestination.Spaces, host.top)
        assertNull(viewModel.state.value.selected)
    }

    @Test
    fun aPackagePageRestoredByTheBackStackSelectsItsPackage() {
        val snapshot = packageSnapshot()
        val client = FakeRuntimeClient(initialPackages = listOf(snapshot))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)
        val moves = mutableListOf<SlotsNavigation>()
        CoroutineScope(Dispatchers.Unconfined).launch {
            viewModel.navigation.collect { moves += it }
        }

        // After process death the restored back stack reports the package page first.
        viewModel.onIntent(
            UiIntent.DestinationChanged(ManagerDestination.PackageDetails, snapshot.packageName),
        )
        assertEquals(snapshot.packageName, viewModel.state.value.selected?.packageName)

        viewModel.onIntent(UiIntent.Refresh)

        assertEquals(snapshot.packageName, viewModel.state.value.selected?.packageName)
        assertTrue(moves.none { it == SlotsNavigation.ToSpaces })
    }

    @Test
    fun anAddAppPageRestoredByTheBackStackLoadsTheCatalog() {
        val apps = RefreshingCatalogSource()
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
        val viewModel = slotsViewModel(client, apps, Dispatchers.Unconfined)
        assertFalse(viewModel.state.value.appCatalogLoaded)

        viewModel.onIntent(UiIntent.DestinationChanged(ManagerDestination.AddApp))

        assertTrue(viewModel.state.value.appCatalogLoaded)
        assertEquals(1, apps.catalogReads)
        assertEquals(
            setOf("com.example.app", "com.other.app"),
            viewModel.state.value.installedApps.map(InstalledApp::packageName).toSet(),
        )
    }

    @Test
    fun anAddAppPageReportedWhileTheFirstRefreshRunsLoadsTheCatalogWhenItEnds() {
        val apps = RefreshingCatalogSource()
        val client = GatedProbeRuntimeClient()
        val viewModel = slotsViewModel(client, apps, Dispatchers.Unconfined)
        assertTrue(viewModel.state.value.busy)

        // Restored after process death: the page is reported before the refresh has ended.
        viewModel.onIntent(UiIntent.DestinationChanged(ManagerDestination.AddApp))
        assertEquals(0, apps.catalogReads)
        client.gate.complete(Unit)

        assertFalse(viewModel.state.value.busy)
        assertTrue(viewModel.state.value.appCatalogLoaded)
        assertEquals(1, apps.catalogReads)
        assertEquals(
            setOf("com.example.app", "com.other.app"),
            viewModel.state.value.installedApps.map(InstalledApp::packageName).toSet(),
        )
    }

    @Test
    fun aRestoredPackagePageReturnsHomeWhenTheRuntimeCannotListIt() {
        listOf(
            RuntimeReply.TransportFailure,
            RuntimeReply.Error(ErrorCode.OperationFailed),
            RuntimeReply.Capabilities("0.0.0-other"),
        ).forEach { probeReply ->
            // Reported while the first refresh still runs ...
            val gated = GatedProbeRuntimeClient(probeReply)
            val during = slotsViewModel(gated, appSource, Dispatchers.Unconfined)
            val duringMoves = recordNavigation(during)
            during.onIntent(
                UiIntent.DestinationChanged(ManagerDestination.PackageDetails, "com.example.app"),
            )
            assertTrue(duringMoves.isEmpty())
            gated.gate.complete(Unit)
            assertEquals(listOf<SlotsNavigation>(SlotsNavigation.ToSpaces), duringMoves.distinct())
            assertNull(during.state.value.selected)

            // ... or after it ended.
            val failed = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
            failed.nextReply = probeReply
            val after = slotsViewModel(failed, appSource, Dispatchers.Unconfined)
            val afterMoves = recordNavigation(after)
            after.onIntent(
                UiIntent.DestinationChanged(ManagerDestination.PackageDetails, "com.example.app"),
            )
            assertEquals(listOf<SlotsNavigation>(SlotsNavigation.ToSpaces), afterMoves.distinct())
        }
    }

    @Test
    fun homeOrderIsEditedInTheViewModelAndSavedOnlyOnSave() {
        val preferences = MemoryManagerUiPreferences()
        val viewModel = slotsViewModel(
            TwoAppsRuntimeClient(),
            twoAppsSource,
            Dispatchers.Unconfined,
            preferences,
        )
        assertEquals(
            listOf("com.example.app", "com.other.app"),
            viewModel.state.value.orderedManagedApps().map(InstalledApp::packageName),
        )

        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.Start))
        assertTrue(viewModel.state.value.reordering)
        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.MoveDown("com.example.app")))
        assertEquals(
            listOf("com.other.app", "com.example.app"),
            viewModel.state.value.reorderKeys(),
        )
        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.Cancel))

        assertFalse(viewModel.state.value.reordering)
        assertEquals(emptyList(), preferences.homeAppOrder)
        assertEquals(
            listOf("com.example.app", "com.other.app"),
            viewModel.state.value.orderedManagedApps().map(InstalledApp::packageName),
        )

        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.Start))
        viewModel.onIntent(
            UiIntent.Reorder(ReorderIntent.MoveTo("com.other.app", "com.example.app")),
        )
        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.Save))

        assertFalse(viewModel.state.value.reordering)
        assertEquals(listOf("com.other.app", "com.example.app"), preferences.homeAppOrder)
        assertEquals(
            listOf("com.other.app", "com.example.app"),
            viewModel.state.value.orderedManagedApps().map(InstalledApp::packageName),
        )
        // A later Manager start reads the saved order.
        val reopened = slotsViewModel(
            TwoAppsRuntimeClient(),
            twoAppsSource,
            Dispatchers.Unconfined,
            preferences,
        )
        assertEquals(
            listOf("com.other.app", "com.example.app"),
            reopened.state.value.orderedManagedApps().map(InstalledApp::packageName),
        )
    }

    @Test
    fun reorderingNeedsMoreThanOneApp() {
        val client = FakeRuntimeClient(initialPackages = listOf(packageSnapshot()))
        val viewModel = slotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.Reorder(ReorderIntent.Start))

        assertFalse(viewModel.state.value.reordering)
    }

    private fun configureApp(viewModel: SlotsViewModel) {
        viewModel.onIntent(UiIntent.RequestConfigureApp("com.example.app"))
        viewModel.onIntent(UiIntent.ConfirmConfigureApp)
    }
}

/**
 * Stands in for the Navigation-Compose host: follows the moves the view model asks for on a
 * simple back stack and reports the page on top, as ManagerNavHost does.
 */
private class FakeNavHost(private val viewModel: SlotsViewModel) {
    private val stack = mutableListOf(ManagerDestination.Spaces)
    val moves = mutableListOf<SlotsNavigation>()
    val top: ManagerDestination
        get() = stack.last()

    init {
        CoroutineScope(Dispatchers.Unconfined).launch {
            viewModel.navigation.collect { follow(it) }
        }
        report()
    }

    /** System or title-bar back: one page, never while an operation runs. */
    fun back() {
        if (viewModel.state.value.busy || stack.size == 1) return
        stack.removeAt(stack.lastIndex)
        report()
    }

    private fun follow(navigation: SlotsNavigation) {
        moves += navigation
        when (navigation) {
            is SlotsNavigation.ToPackage -> if (top != ManagerDestination.PackageDetails) {
                popToSpaces()
                stack += ManagerDestination.PackageDetails
            }
            SlotsNavigation.ToAddApps -> {
                popToSpaces()
                stack += ManagerDestination.AddApp
            }
            SlotsNavigation.ToRuntimeStatus -> {
                popToSpaces()
                stack += ManagerDestination.RuntimeStatus
            }
            is SlotsNavigation.ToBackupRestore -> if (top != ManagerDestination.BackupRestore) {
                stack += ManagerDestination.BackupRestore
            }
            SlotsNavigation.ToSpaces -> popToSpaces()
        }
        report()
    }

    private fun popToSpaces() {
        while (stack.size > 1) stack.removeAt(stack.lastIndex)
    }

    private fun report() {
        viewModel.onIntent(UiIntent.DestinationChanged(top))
    }
}

private fun recordNavigation(viewModel: SlotsViewModel): MutableList<SlotsNavigation> {
    val moves = mutableListOf<SlotsNavigation>()
    CoroutineScope(Dispatchers.Unconfined).launch {
        viewModel.navigation.collect { moves += it }
    }
    return moves
}

/** Holds the first Probe until [gate] completes, then answers it with [probeReply] if set. */
private class GatedProbeRuntimeClient(
    private val probeReply: RuntimeReply? = null,
) : FakeRuntimeClient(initialPackages = listOf(packageSnapshot())) {
    val gate = CompletableDeferred<Unit>()
    private var gated = true

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        if (command == RuntimeCommand.Probe && gated) {
            gated = false
            gate.await()
            if (probeReply != null) {
                commands += command
                return probeReply
            }
        }
        return super.execute(command)
    }
}

private val twoAppsSource = InstalledAppsSource {
    listOf(
        InstalledApp("com.example.app", "Example", signingIdentity = testSigning()),
        InstalledApp("com.other.app", "Other", signingIdentity = testSigning()),
    )
}

private class TwoAppsRuntimeClient : FakeRuntimeClient(initialPackages = listOf(packageSnapshot())) {
    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        if (command == RuntimeCommand.ListPackages) {
            commands += command
            RuntimeReply.Packages(
                listOf(snapshot, snapshot.copy(packageName = "com.other.app")),
            )
        } else {
            super.execute(command)
        }
}

private val TEST_LABELS = SpaceLabels(baseSpaceName = "系统原始空间", bindingConfirmation = "绑定")

private fun slotsViewModel(
    client: RuntimeClient,
    apps: InstalledAppsSource,
    dispatcher: CoroutineDispatcher,
    preferences: ManagerUiPreferences = MemoryManagerUiPreferences(),
    maintenance: RuntimeMaintenance = NoRuntimeMaintenance(),
) = SlotsViewModel(client, apps, dispatcher, preferences, maintenance, TEST_LABELS)

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

private val TEST_RUNTIME_VERSION = BuildConfig.RUNTIME_PROTOCOL_BUILD_ID

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
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(buildId, buildId, launchGateReady = true, processViewReady = true)
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
                    defaultSlot = snapshot.defaultSlot.takeUnless { it == command.slotId },
                )
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.Unenroll -> {
                enrolled = false
                RuntimeReply.Ack
            }
            is RuntimeCommand.SetDefaultSlot -> {
                snapshot = snapshot.copy(defaultSlot = command.slotId)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.SetLaunchAfterReboot -> {
                snapshot = snapshot.copy(launchAfterReboot = command.enabled)
                RuntimeReply.Package(snapshot)
            }
            is RuntimeCommand.BeginBackupIo,
            is RuntimeCommand.FinishBackupIo,
            is RuntimeCommand.BeginRestoreIo,
            is RuntimeCommand.CommitRestoreAccount,
            is RuntimeCommand.FinishRestoreIo,
            is RuntimeCommand.AbortAccountIo,
            RuntimeCommand.ListAccountIoStatus,
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
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
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION, TEST_RUNTIME_VERSION, launchGateReady = true, processViewReady = true)
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
            is RuntimeCommand.SetDefaultSlot,
            is RuntimeCommand.BeginBackupIo,
            is RuntimeCommand.FinishBackupIo,
            is RuntimeCommand.BeginRestoreIo,
            is RuntimeCommand.CommitRestoreAccount,
            is RuntimeCommand.FinishRestoreIo,
            is RuntimeCommand.AbortAccountIo,
            RuntimeCommand.ListAccountIoStatus,
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
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION, TEST_RUNTIME_VERSION, launchGateReady = true, processViewReady = true)
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
            is RuntimeCommand.SetDefaultSlot,
            is RuntimeCommand.BeginBackupIo,
            is RuntimeCommand.FinishBackupIo,
            is RuntimeCommand.BeginRestoreIo,
            is RuntimeCommand.CommitRestoreAccount,
            is RuntimeCommand.FinishRestoreIo,
            is RuntimeCommand.AbortAccountIo,
            RuntimeCommand.ListAccountIoStatus,
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
            RuntimeCommand.Probe -> RuntimeReply.Capabilities(TEST_RUNTIME_VERSION, TEST_RUNTIME_VERSION, launchGateReady = true, processViewReady = true)
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
            is RuntimeCommand.SetDefaultSlot,
            is RuntimeCommand.BeginBackupIo,
            is RuntimeCommand.FinishBackupIo,
            is RuntimeCommand.BeginRestoreIo,
            is RuntimeCommand.CommitRestoreAccount,
            is RuntimeCommand.FinishRestoreIo,
            is RuntimeCommand.AbortAccountIo,
            RuntimeCommand.ListAccountIoStatus,
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
}

private class FakeRuntimeMaintenance(
    private val restarts: Boolean,
    private val log: String? = null,
) : RuntimeMaintenance {
    var restartCalls = 0
    var beforeLog: (suspend () -> Unit)? = null

    override suspend fun restartRuntime(): Boolean {
        restartCalls += 1
        return restarts
    }

    override suspend fun readLogTail(): String? {
        beforeLog?.invoke()
        return log
    }
}

private class HoldingListRuntimeClient : FakeRuntimeClient(
    initialPackages = listOf(packageSnapshot()),
) {
    var holdList: CompletableDeferred<Unit>? = null

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        if (command == RuntimeCommand.ListPackages) holdList?.await()
        return super.execute(command)
    }
}

private class RefreshingCatalogSource : InstalledAppsSource, OptimizedInstalledAppsSource {
    var duringFirstCatalog: (() -> Unit)? = null
    var catalogReads = 0

    override fun load(): List<InstalledApp> = loadCatalog()

    override fun loadPackages(packageNames: Set<String>): List<InstalledApp> =
        catalog().filter { it.packageName in packageNames }

    override fun loadCatalog(): List<InstalledApp> {
        catalogReads += 1
        duringFirstCatalog?.let { refresh ->
            duringFirstCatalog = null
            refresh()
        }
        return catalog()
    }

    private fun catalog() = listOf(
        InstalledApp("com.example.app", "Example", signingIdentity = testSigning()),
        InstalledApp("com.other.app", "Other"),
    )
}

private class BlockingGetPackageRuntimeClient : FakeRuntimeClient(
    initialPackages = listOf(packageSnapshot()),
) {
    val getStarted = CompletableDeferred<Unit>()
    val releaseGet = CompletableDeferred<Unit>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        if (command is RuntimeCommand.GetPackage) {
            snapshot = snapshot.copy(activeSlot = "slot-1")
            getStarted.complete(Unit)
            releaseGet.await()
        }
        return super.execute(command)
    }
}
