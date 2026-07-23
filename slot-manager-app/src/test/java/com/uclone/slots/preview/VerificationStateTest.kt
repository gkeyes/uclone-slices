package com.uclone.slots.preview

import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.model.VerificationSource
import com.uclone.slots.preview.model.VerificationState
import com.uclone.slots.preview.model.VerifiedPackageSnapshot
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class VerificationStateTest {
    private val snapshot = VerifiedPackageSnapshot(
        status = PackageRuntimeStatus(
            packageName = "com.example.app",
            activeSlot = "work",
            lifecycle = PackageLifecycle.Normal,
            enabled = false,
            suspended = false,
        ),
        slots = listOf(
            SlotSpace("work", "Work", "blank", "ready", true, 101, 201),
        ),
    )

    @Test
    fun reconcileFailureRevokesPreviouslyDisabledProof() {
        val oldProof = VerificationState.Verified(snapshot, VerificationSource.Snapshot)

        assertEquals(
            VerificationState.Unverified,
            verificationAfterUnknown(RuntimeResult.Unknown("reconcile timeout"), oldProof),
        )
        assertEquals(
            VerificationState.Unverified,
            verificationAfterUnknown(RuntimeResult.Rejected("internal"), oldProof),
        )
    }

    @Test
    fun matchingSwitchAndFreshSnapshotAllowLaunchInOrder() {
        val switch = RuntimeResult.Success(RuntimePayload.Switch("com.example.app", "work"))
        val postMutation = VerificationState.Verified(snapshot, VerificationSource.PostMutation)

        assertTrue(canLaunchAfterSwitch(switch, postMutation, "com.example.app", "work"))
    }

    @Test
    fun mismatchAndTimeoutNeverAllowLaunch() {
        val postMutation = VerificationState.Verified(snapshot, VerificationSource.PostMutation)
        val mismatch = RuntimeResult.Success(RuntimePayload.Switch("com.example.app", "other"))

        assertFalse(canLaunchAfterSwitch(mismatch, postMutation, "com.example.app", "work"))
        assertFalse(
            canLaunchAfterSwitch(
                RuntimeResult.Unknown("switch timeout"),
                postMutation,
                "com.example.app",
                "work",
            ),
        )
        assertFalse(
            canLaunchAfterSwitch(
                RuntimeResult.Success(RuntimePayload.Switch("com.example.app", "work")),
                VerificationState.Unverified,
                "com.example.app",
                "work",
            ),
        )
    }

    @Test
    fun ordinarySnapshotIsNotEnoughForPostMutationLaunch() {
        val switch = RuntimeResult.Success(RuntimePayload.Switch("com.example.app", "work"))
        val ordinary = VerificationState.Verified(snapshot, VerificationSource.Snapshot)

        assertFalse(canLaunchAfterSwitch(switch, ordinary, "com.example.app", "work"))
    }
}
