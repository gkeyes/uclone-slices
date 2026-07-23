package com.uclone.slots.preview

import com.uclone.slots.preview.model.RuntimeHealth
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import kotlin.test.Test
import kotlin.test.assertEquals

class UiMappingsTest {
    @Test
    fun rescueOnlyProbeKeepsRecoveryReachable() {
        val probe = RuntimePayload.Probe(
            ready = false,
            userUnlocked = true,
            ceDeSupported = true,
            runtimeVersion = "test",
            buildId = "test",
            recoveryOnly = true,
        )

        assertEquals(RuntimeHealth.RecoveryRequired, UiMappings.probeHealth(probe))
    }

    @Test
    fun recoveryOnlyRuntimeDoesNotNeedToClaimCeIsUnlockedToExposeRescue() {
        val probe = RuntimePayload.Probe(false, false, true, "test", "test", recoveryOnly = true)

        assertEquals(RuntimeHealth.RecoveryRequired, UiMappings.probeHealth(probe))
    }

    @Test
    fun ordinaryLockedRuntimeIsNotMisreportedAsRecoveryOnly() {
        val probe = RuntimePayload.Probe(false, false, true, "test", "test")

        val state = UiMappings.probeState(RuntimeResult.Success(probe))

        assertEquals(RuntimeHealth.UserLocked, state.health)
        assertEquals(com.uclone.slots.preview.model.RuntimeMode.Ordinary, state.mode)
    }

    @Test
    fun pairingMismatchIsNotReportedAsMissingModule() {
        assertEquals(
            RuntimeHealth.PairMismatch,
            UiMappings.healthForError("runtime_pair_mismatch"),
        )
    }

    @Test
    fun freshManagerCanRenderRuntimeRecoveryTargetsWithoutCachedState() {
        val rows = recoveryManagedApps(
            listOf("com.example.one", "com.example.two"),
        ) { "label:$it" }

        assertEquals(2, rows.size)
        assertEquals("unknown", rows.first().activeSlot)
        assertEquals(PackageLifecycle.RecoveryRequired, rows.first().lifecycle)
        assertEquals("label:com.example.one", rows.first().label)
    }
}
