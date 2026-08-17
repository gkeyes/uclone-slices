package com.uclone.slices.v2.desktop

import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull

class DesktopShortcutProjectionTest {
    @Test
    fun nextTitleIsOnlyTheTargetSlicesNameForAllThreeDirections() {
        val projection = MemoryDesktopShortcutProjection()

        projection.upsert(snapshot(active = "base"))
        assertEquals("工作微信", projection.menuState(PACKAGE)?.targetName)

        projection.upsert(snapshot(active = "slot-1"))
        assertEquals(SYSTEM_ORIGINAL_SPACE, projection.menuState(PACKAGE)?.targetName)

        projection.upsert(snapshot(active = "slot-2"))
        assertEquals("工作微信", projection.menuState(PACKAGE)?.targetName)
    }

    @Test
    fun renameRebindAndDeleteSynchronizeTheProjection() {
        val projection = MemoryDesktopShortcutProjection()
        projection.upsert(snapshot())

        projection.upsert(snapshot(boundName = "新的工作微信"))
        assertEquals("新的工作微信", projection.menuState(PACKAGE)?.targetName)

        projection.upsert(snapshot(bound = "slot-2", active = "slot-1"))
        assertEquals("第三账号", projection.menuState(PACKAGE)?.targetName)

        projection.upsert(snapshot(bound = null))
        assertNull(projection.menuState(PACKAGE))
    }

    @Test
    fun nonReadyOrInvalidBindingsAreNeverProjected() {
        val projection = MemoryDesktopShortcutProjection()

        projection.upsert(snapshot(bindingState = BindingState.RebindRequired))
        assertNull(projection.menuState(PACKAGE))

        projection.upsert(snapshot(bound = "slot-9"))
        assertNull(projection.menuState(PACKAGE))
    }

    @Test
    fun quickSwitchChecksVersionAndUpdatesProjectionOnlyOnSuccess() = runBlocking {
        val projection = MemoryDesktopShortcutProjection().apply { upsert(snapshot()) }
        val successSnapshot = snapshot(active = "slot-1")
        val client = ReplyClient(
            RuntimeReply.Capabilities("0.1.8"),
            RuntimeReply.Package(successSnapshot),
        )
        val runner = DesktopQuickSwitchRunner(client, projection, "0.1.8")

        val result = runner.run(PACKAGE)

        assertEquals(DesktopQuickSwitchResult.Success("工作微信"), result)
        assertEquals(SYSTEM_ORIGINAL_SPACE, projection.menuState(PACKAGE)?.targetName)
        assertEquals(
            listOf(RuntimeCommand.Probe, RuntimeCommand.ActivateDesktopShortcut(PACKAGE)),
            client.commands,
        )
    }

    @Test
    fun versionMismatchAndRuntimeFailurePreserveThePreviousProjection() = runBlocking {
        val projection = MemoryDesktopShortcutProjection().apply { upsert(snapshot()) }
        val mismatchClient = ReplyClient(RuntimeReply.Capabilities("0.1.7"))

        val mismatch = DesktopQuickSwitchRunner(mismatchClient, projection, "0.1.8")
            .run(PACKAGE)

        assertIs<DesktopQuickSwitchResult.VersionMismatch>(mismatch)
        assertEquals("工作微信", projection.menuState(PACKAGE)?.targetName)
        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.Probe), mismatchClient.commands)

        val failedClient = ReplyClient(
            RuntimeReply.Capabilities("0.1.8"),
            RuntimeReply.Error(ErrorCode.OperationFailed),
        )
        val failed = DesktopQuickSwitchRunner(failedClient, projection, "0.1.8")
            .run(PACKAGE)

        assertEquals(DesktopQuickSwitchResult.Failed(ErrorCode.OperationFailed), failed)
        assertEquals("工作微信", projection.menuState(PACKAGE)?.targetName)
    }

    private fun snapshot(
        active: String = "base",
        bound: String? = "slot-1",
        boundName: String = "工作微信",
        bindingState: BindingState = BindingState.Ready,
    ) = PackageSnapshot(
        packageName = PACKAGE,
        activeSlot = active,
        slots = listOf(
            SlotSnapshot("base", "Base"),
            SlotSnapshot("slot-1", boundName),
            SlotSnapshot("slot-2", "第三账号"),
        ),
        desktopShortcutSlot = bound,
        bindingState = bindingState,
    )

    private companion object {
        const val PACKAGE = "com.tencent.mm"
    }
}

private class ReplyClient(vararg replies: RuntimeReply) : RuntimeClient {
    private val replies = ArrayDeque(replies.toList())
    val commands = mutableListOf<RuntimeCommand>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        commands += command
        return replies.removeFirst()
    }
}
