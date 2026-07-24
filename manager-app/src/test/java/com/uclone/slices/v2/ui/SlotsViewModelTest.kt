package com.uclone.slices.v2.ui

import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SeedMode
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SlotsViewModelTest {
    private val appSource = InstalledAppsSource {
        listOf(InstalledApp("com.example.app", "Example"))
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
    }

    @Test
    fun localAppSelectionEnrollsThenOpensAnExistingPackage() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))

        assertEquals(
            listOf(
                RuntimeCommand.Probe,
                RuntimeCommand.ListPackages,
                RuntimeCommand.Enroll("com.example.app"),
                RuntimeCommand.GetPackage("com.example.app"),
            ),
            client.commands,
        )
    }

    @Test
    fun backReturnsToTheAppListWithoutCallingRuntime() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        val beforeBack = client.commands.toList()

        viewModel.onIntent(UiIntent.BackToPackages)

        assertEquals(null, viewModel.state.value.selected)
        assertEquals(beforeBack, client.commands)
    }

    @Test
    fun operationArgumentsAreFrozenAtIntentTime() = runBlocking {
        val client = FreezingRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
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
    fun blankSlotNameNeverReachesRuntime() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))
        viewModel.onIntent(UiIntent.SlotNameChanged("   "))
        val commandsBeforeCreate = client.commands.toList()

        viewModel.onIntent(UiIntent.CreateSlot)

        assertEquals(commandsBeforeCreate, client.commands)
    }

    @Test
    fun busyDropsASecondOperationInsteadOfQueuingIt() = runBlocking {
        val client = BlockingRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)

        viewModel.onIntent(UiIntent.OpenPackage("com.example.first"))
        client.firstStarted.await()
        assertTrue(viewModel.state.value.busy)
        viewModel.onIntent(UiIntent.OpenPackage("com.example.second"))

        assertEquals(1, client.openCommands.size)
        client.releaseFirst.complete(Unit)
        assertFalse(viewModel.state.value.busy)
        assertEquals(
            listOf<RuntimeCommand>(RuntimeCommand.Enroll("com.example.first")),
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
    }

    @Test
    fun fourWireErrorsHaveDistinctMessagesAndKeepTheConnectionState() {
        val expected = mapOf(
            ErrorCode.InvalidRequest to "输入无效",
            ErrorCode.NotFound to "应用或数据槽不存在",
            ErrorCode.StateConflict to "应用记录与当前数据状态不一致，请刷新后重试",
            ErrorCode.OperationFailed to "Runtime 操作失败",
        )
        expected.forEach { (code, message) ->
            val client = FakeRuntimeClient()
            val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
            client.nextReply = RuntimeReply.Error(code)

            viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))

            assertEquals(message, viewModel.state.value.message)
            assertTrue(viewModel.state.value.runtimeReady)
        }
    }

    @Test
    fun transportFailureClearsRuntimeReady() {
        val client = FakeRuntimeClient()
        val viewModel = SlotsViewModel(client, appSource, Dispatchers.Unconfined)
        client.nextReply = RuntimeReply.TransportFailure

        viewModel.onIntent(UiIntent.OpenPackage("com.example.app"))

        assertEquals("Runtime 未连接", viewModel.state.value.message)
        assertFalse(viewModel.state.value.runtimeReady)
    }
}

private fun packageSnapshot(active: String = "base") = PackageSnapshot(
    packageName = "com.example.app",
    activeSlot = active,
    slots = listOf(
        SlotSnapshot("base", "Base"),
        SlotSnapshot("slot-1", "Work"),
    ),
)

private open class FakeRuntimeClient(
    initialPackages: List<PackageSnapshot> = emptyList(),
) : RuntimeClient {
    val commands = mutableListOf<RuntimeCommand>()
    var snapshot = initialPackages.firstOrNull() ?: packageSnapshot()
    var nextReply: RuntimeReply? = null

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        commands += command
        nextReply?.let {
            nextReply = null
            return it
        }
        return when (command) {
            RuntimeCommand.Probe -> RuntimeReply.Capabilities("test")
            RuntimeCommand.ListPackages -> RuntimeReply.Packages(
                if (commands.count { it == RuntimeCommand.ListPackages } == 1) {
                    emptyList()
                } else {
                    listOf(snapshot)
                },
            )
            is RuntimeCommand.GetPackage,
            is RuntimeCommand.Enroll,
            -> RuntimeReply.Package(snapshot)
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
            RuntimeCommand.Probe -> RuntimeReply.Capabilities("test")
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
            -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
}
