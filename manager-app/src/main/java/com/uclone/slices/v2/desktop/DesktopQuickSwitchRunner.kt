package com.uclone.slices.v2.desktop

import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply

internal sealed interface DesktopQuickSwitchResult {
    data class Success(val targetName: String) : DesktopQuickSwitchResult
    data object RuntimeUnavailable : DesktopQuickSwitchResult
    data object VersionMismatch : DesktopQuickSwitchResult
    data class Failed(val code: ErrorCode) : DesktopQuickSwitchResult
}

internal class DesktopQuickSwitchRunner(
    private val client: RuntimeClient,
    private val projection: DesktopShortcutProjection,
    private val expectedRuntimeVersion: String,
) {
    suspend fun run(packageName: String): DesktopQuickSwitchResult {
        when (val probe = client.execute(RuntimeCommand.Probe)) {
            is RuntimeReply.Capabilities -> if (probe.buildId != expectedRuntimeVersion) {
                return DesktopQuickSwitchResult.VersionMismatch
            }
            RuntimeReply.TransportFailure -> return DesktopQuickSwitchResult.RuntimeUnavailable
            is RuntimeReply.Error -> return DesktopQuickSwitchResult.Failed(probe.code)
            RuntimeReply.Ack,
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            -> return DesktopQuickSwitchResult.Failed(ErrorCode.OperationFailed)
        }

        return when (
            val reply = client.execute(RuntimeCommand.ActivateDesktopShortcut(packageName))
        ) {
            is RuntimeReply.Package -> {
                projection.upsert(reply.packageSnapshot)
                val targetName = reply.packageSnapshot.slots
                    .firstOrNull { it.id == reply.packageSnapshot.activeSlot }
                    ?.let { slot ->
                        if (slot.id == BASE_SLOT_ID) SYSTEM_ORIGINAL_SPACE else slot.name
                    }
                    ?: reply.packageSnapshot.activeSlot
                DesktopQuickSwitchResult.Success(targetName)
            }
            RuntimeReply.TransportFailure -> DesktopQuickSwitchResult.RuntimeUnavailable
            is RuntimeReply.Error -> DesktopQuickSwitchResult.Failed(reply.code)
            RuntimeReply.Ack,
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            -> DesktopQuickSwitchResult.Failed(ErrorCode.OperationFailed)
        }
    }
}
