package com.uclone.slots.preview.ui

import com.uclone.slots.preview.launchStatusMessage
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.model.RuntimeHealth
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SlotActionPolicyTest {
    private val slotA = slot("slot_a", active = true)
    private val slotB = slot("slot_b", active = false)

    @Test
    fun everyReadySlotCanSwitchAndOpenWithoutReturningToBase() {
        assertTrue(canSwitchAndOpen(enabled = true, slot = slotA))
        assertTrue(canSwitchAndOpen(enabled = true, slot = slotB))
        assertFalse(canSwitchAndOpen(enabled = false, slot = slotB))
        assertFalse(canSwitchAndOpen(enabled = true, slot = slotB.copy(state = "creating")))
    }

    @Test
    fun createAndDeleteRemainRestrictedToBase() {
        assertTrue(canCreateSpace(enabled = true, baseActive = true))
        assertFalse(canCreateSpace(enabled = true, baseActive = false))
        assertTrue(canDeleteSlot(enabled = true, baseActive = true, slot = slotB))
        assertFalse(canDeleteSlot(enabled = true, baseActive = false, slot = slotB))
        assertFalse(canDeleteSlot(enabled = true, baseActive = true, slot = slotA))
        assertFalse(canDeleteSlot(enabled = true, baseActive = true, slot = base()))
    }

    @Test
    fun cachedAppsExposeOnlyTheIndependentRecoverySurfaceWhenRuntimeIsUnavailable() {
        for (health in listOf(
            RuntimeHealth.DaemonOffline,
            RuntimeHealth.PairMismatch,
            RuntimeHealth.RecoveryRequired,
        )) {
            assertTrue(canOpenManagedAppDetails(health))
            assertTrue(canUseIndependentBaseRescue(health, busy = false))
            assertTrue(canSelectIndependentBaseRescueTarget(health))
            assertFalse(canUseOrdinarySlotActions(health, busy = false, packageSafe = true))
        }
        for (health in listOf(RuntimeHealth.ModuleMissing, RuntimeHealth.Checking)) {
            assertFalse(canOpenManagedAppDetails(health))
            assertFalse(canUseIndependentBaseRescue(health, busy = false))
            assertFalse(canSelectIndependentBaseRescueTarget(health))
        }
        assertFalse(canUseReconcile(RuntimeHealth.DaemonOffline, busy = false))
        assertFalse(canUseReconcile(RuntimeHealth.PairMismatch, busy = false))
        assertTrue(canUseReconcile(RuntimeHealth.RecoveryRequired, busy = false))
    }

    @Test
    fun createAndSwitchShareTheTypedLaunchOutcomeMessages() {
        assertTrue(launchStatusMessage("launched") == null)
        assertTrue(launchStatusMessage("entry_not_found")?.contains("没有可启动入口") == true)
        assertTrue(launchStatusMessage("gate_blocked")?.contains("原本不可启动") == true)
        assertTrue(launchStatusMessage("failed")?.contains("启动请求失败") == true)
    }

    private fun slot(id: String, active: Boolean) = SlotSpace(
        id = id,
        displayName = id,
        seedMode = "blank",
        state = "ready",
        active = active,
        ceInode = 101,
        deInode = 201,
    )

    private fun base() = slot("base", active = false)
}
