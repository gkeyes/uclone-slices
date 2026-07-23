package com.uclone.slots.preview

import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.model.SlotsUiState
import com.uclone.slots.preview.model.VerificationSource
import com.uclone.slots.preview.model.VerificationState
import com.uclone.slots.preview.model.VerifiedPackageSnapshot
import com.uclone.slots.preview.model.revokeVerification
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SlotsUiStateTest {
    @Test
    fun revokeClearsStatusSlotsAndVerificationAsOneStateTransition() {
        val status = PackageRuntimeStatus(
            packageName = "com.example.app",
            activeSlot = "work",
            lifecycle = PackageLifecycle.Normal,
            enabled = false,
            suspended = true,
        )
        val slots = listOf(SlotSpace("work", "Work", "blank", "ready", true, 10, 20))
        val state = SlotsUiState(
            selectedStatus = status,
            selectedSlots = slots,
            verificationState = VerificationState.Verified(
                VerifiedPackageSnapshot(status, slots),
                VerificationSource.PostMutation,
            ),
        )

        val revoked = state.revokeVerification()

        assertEquals(VerificationState.Unverified, revoked.verificationState)
        assertNull(revoked.selectedStatus)
        assertEquals(emptyList(), revoked.selectedSlots)
    }
}
