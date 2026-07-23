package com.uclone.slots.preview

import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.model.SlotsUiState
import com.uclone.slots.preview.runtime.RuntimeGateway
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking

class SlotMutationFlowTest {
    @Test
    fun switchUsesFreshPostMutationSnapshotBeforeLaunch() = runBlocking {
        val gateway = FakeGateway()
        val coordinator = SlotOperationCoordinator(
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
            reduce = { reducer -> reducer(SlotsUiState()) },
        )
        var verified = false
        var launched = false

        coordinator.executeMutationAndLaunch(
            runtime = gateway,
            packageName = PACKAGE,
            requestedSlot = SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { snapshot, slotId, _ ->
                verified = snapshot.status.activeSlot == slotId &&
                    snapshot.slots.single { it.active }.id == slotId
                verified
            },
            onLaunch = { result, slotId, _ ->
                launched = result is RuntimeResult.Success && slotId == SLOT
            },
            onFailure = { error("unexpected failure: $it") },
        )

        assertEquals(listOf("switch", "package_snapshot", "launch_current"), gateway.calls)
        assertTrue(verified)
        assertTrue(launched)
    }

    @Test
    fun unknownSwitchNeverLaunches() = runBlocking {
        val gateway = FakeGateway(switchResult = RuntimeResult.Unknown("timeout"))
        val coordinator = SlotOperationCoordinator(
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
            reduce = { reducer -> reducer(SlotsUiState()) },
        )
        var failures = 0
        var launches = 0

        coordinator.executeMutationAndLaunch(
            runtime = gateway,
            packageName = PACKAGE,
            requestedSlot = SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { _, _, _ -> true },
            onLaunch = { _, _, _ -> launches += 1 },
            onFailure = { failures += 1 },
        )

        assertEquals(listOf("switch"), gateway.calls)
        assertEquals(1, failures)
        assertEquals(0, launches)
    }

    @Test
    fun explicitBaseSwitchStillUsesSnapshotBeforeLaunch() = runBlocking {
        val gateway = FakeGateway(
            switchResult = RuntimeResult.Success(RuntimePayload.Switch(PACKAGE, "base")),
        )
        val coordinator = coordinator()
        var launches = 0

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            "base",
            mutation = { gateway.switchSlot(PACKAGE, "base") },
            onSnapshot = { _, _, _ -> true },
            onLaunch = { _, _, _ -> launches += 1 },
            onFailure = { error("unexpected failure: $it") },
        )

        assertEquals(listOf("switch", "package_snapshot", "launch_current"), gateway.calls)
        assertEquals(1, launches)
    }

    @Test
    fun createCannotAcceptBaseAsTheNewSpace() = runBlocking {
        val gateway = FakeGateway(
            createResult = RuntimeResult.Success(RuntimePayload.Switch(PACKAGE, "base")),
        )
        val coordinator = coordinator()
        var failures = 0

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            null,
            mutation = { gateway.createSlot(PACKAGE, "name", blank = true) },
            onSnapshot = { _, _, _ -> true },
            onLaunch = { _, _, _ -> error("base create launched") },
            onFailure = { failures += 1 },
        )

        assertEquals(listOf("create"), gateway.calls)
        assertEquals(1, failures)
    }

    @Test
    fun rejectedAndUnknownMutationsArePreservedWithoutReads() = runBlocking {
        for (failure in listOf(RuntimeResult.Rejected("conflict"), RuntimeResult.Unknown("timeout"))) {
            val gateway = FakeGateway(switchResult = failure)
            val coordinator = coordinator()
            var received: RuntimeResult? = null

            coordinator.executeMutationAndLaunch(
                gateway,
                PACKAGE,
                SLOT,
                mutation = { gateway.switchSlot(PACKAGE, SLOT) },
                onSnapshot = { _, _, _ -> true },
                onLaunch = { _, _, _ -> error("failed mutation launched") },
                onFailure = { received = it },
            )

            assertEquals(failure, received)
            assertEquals(listOf("switch"), gateway.calls)
        }
    }

    @Test
    fun recoverySnapshotStopsBeforeLaunch() = runBlocking {
        val gateway = FakeGateway(snapshotLifecycle = PackageLifecycle.RecoveryRequired)
        val coordinator = coordinator()
        var failures = 0

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { _, _, _ -> error("recovery snapshot was accepted") },
            onLaunch = { _, _, _ -> error("recovery snapshot launched") },
            onFailure = { failures += 1 },
        )

        assertEquals(listOf("switch", "package_snapshot"), gateway.calls)
        assertEquals(1, failures)
    }

    @Test
    fun mismatchedSnapshotStopsBeforeLaunch() = runBlocking {
        val gateway = FakeGateway(snapshotSlotOverride = "other")
        val coordinator = coordinator()
        var failures = 0

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { _, _, _ -> error("mismatched snapshot was accepted") },
            onLaunch = { _, _, _ -> error("mismatched snapshot launched") },
            onFailure = { failures += 1 },
        )

        assertEquals(listOf("switch", "package_snapshot"), gateway.calls)
        assertEquals(1, failures)
    }

    @Test
    fun unknownLaunchRoutesToFailureWithoutSuccessCallback() = runBlocking {
        val gateway = FakeGateway(launchResult = RuntimeResult.Unknown("timeout"))
        val coordinator = coordinator()
        var failures = 0
        var successfulLaunches = 0

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { _, _, _ -> true },
            onLaunch = { _, _, _ -> successfulLaunches += 1 },
            onFailure = { failures += 1 },
        )

        assertEquals(listOf("switch", "package_snapshot", "launch_current"), gateway.calls)
        assertEquals(1, failures)
        assertEquals(0, successfulLaunches)
    }

    @Test
    fun rejectedLaunchRoutesToFailureWithoutSuccessCallback() = runBlocking {
        val gateway = FakeGateway(launchResult = RuntimeResult.Rejected("conflict"))
        val coordinator = coordinator()
        var received: RuntimeResult? = null

        coordinator.executeMutationAndLaunch(
            gateway,
            PACKAGE,
            SLOT,
            mutation = { gateway.switchSlot(PACKAGE, SLOT) },
            onSnapshot = { _, _, _ -> true },
            onLaunch = { _, _, _ -> error("rejected launch reported success") },
            onFailure = { received = it },
        )

        assertEquals(RuntimeResult.Rejected("conflict"), received)
        assertEquals(listOf("switch", "package_snapshot", "launch_current"), gateway.calls)
    }

    private fun coordinator() = SlotOperationCoordinator(
        scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
        reduce = { reducer -> reducer(SlotsUiState()) },
    )

    private companion object {
        const val PACKAGE = "com.example.app"
        const val SLOT = "work"
    }
}

private class FakeGateway(
    private val switchResult: RuntimeResult = RuntimeResult.Success(
        RuntimePayload.Switch("com.example.app", "work"),
    ),
    private val createResult: RuntimeResult? = null,
    private val launchResult: RuntimeResult = RuntimeResult.Success(
        RuntimePayload.Launch("com.example.app", "work", "launched"),
    ),
    snapshotLifecycle: PackageLifecycle = PackageLifecycle.Normal,
    snapshotSlotOverride: String? = null,
) : RuntimeGateway {
    val calls = mutableListOf<String>()
    private val snapshotSlot = snapshotSlotOverride ?:
        (((switchResult as? RuntimeResult.Success)?.payload as? RuntimePayload.Switch)?.slotId ?: "work")
    private val snapshot = RuntimePayload.PackageSnapshot(
        status = PackageRuntimeStatus(
            packageName = "com.example.app",
            activeSlot = snapshotSlot,
            lifecycle = snapshotLifecycle,
            enabled = false,
            suspended = false,
        ),
        slots = listOf(SlotSpace(snapshotSlot, snapshotSlot, "blank", "ready", true, 1, 2)),
    )

    override suspend fun probe() = RuntimeResult.Success(RuntimePayload.Probe(true, true, true, "t", "t"))
    override suspend fun inspect(packageName: String) = RuntimeResult.Success(RuntimePayload.Ack("inspect"))
    override suspend fun listManaged() = RuntimeResult.Success(RuntimePayload.ManagedApps(emptyList()))
    override suspend fun listRecoveryTargets() = RuntimeResult.Success(RuntimePayload.RecoveryTargets(emptyList()))
    override suspend fun enroll(packageName: String, acceptDirectBootConditional: Boolean) =
        RuntimeResult.Success(RuntimePayload.Ack("enroll_package"))
    override suspend fun status(packageName: String) = RuntimeResult.Success(RuntimePayload.Status(snapshot.status))
    override suspend fun listSlots(packageName: String) = RuntimeResult.Success(RuntimePayload.Slots(packageName, snapshot.slots))
    override suspend fun packageSnapshot(packageName: String): RuntimeResult {
        calls += "package_snapshot"
        return RuntimeResult.Success(snapshot)
    }
    override suspend fun createSlot(packageName: String, name: String, blank: Boolean): RuntimeResult {
        calls += "create"
        return createResult ?: switchResult
    }
    override suspend fun switchSlot(packageName: String, slotId: String): RuntimeResult {
        calls += "switch"
        return switchResult
    }
    override suspend fun launchCurrent(packageName: String, expectedSlot: String): RuntimeResult {
        calls += "launch_current"
        return launchResult
    }
    override suspend fun rename(packageName: String, slotId: String, name: String) = RuntimeResult.Success(RuntimePayload.Ack("rename_slot"))
    override suspend fun delete(packageName: String, slotId: String) = RuntimeResult.Success(RuntimePayload.Ack("delete_slot"))
    override suspend fun reconcile(packageName: String) = RuntimeResult.Success(RuntimePayload.Reconcile(packageName, "ok"))
    override suspend fun rescue(packageName: String) = RuntimeResult.Success(RuntimePayload.Ack("rescue_to_base"))
}
