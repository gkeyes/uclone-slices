package com.uclone.slots.preview

import com.uclone.slots.preview.model.VerificationSource
import com.uclone.slots.preview.model.VerificationState
import com.uclone.slots.preview.model.VerifiedPackageSnapshot
import com.uclone.slots.preview.runtime.PackageSnapshot
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult

internal fun PackageSnapshot.toVerificationState(
    source: VerificationSource,
): VerificationState = VerificationState.Verified(
    snapshot = VerifiedPackageSnapshot(status, slots),
    source = source,
)

internal fun SlotsViewModel.applyVerifiedSnapshot(
    snapshot: PackageSnapshot,
    source: VerificationSource,
) {
    selectedStatus = snapshot.status
    selectedSlots = snapshot.slots
    markPackageLifecycle(snapshot.status.packageName, snapshot.status.lifecycle)
    verificationState = if (snapshot.status.requiresRecovery) {
        VerificationState.Unverified
    } else {
        snapshot.toVerificationState(source)
    }
}

internal fun SlotsViewModel.revokePackageVerification() {
    verificationState = VerificationState.Unverified
    selectedStatus = null
    selectedSlots = emptyList()
}

internal fun verificationAfterUnknown(
    reconciled: RuntimeResult,
    current: VerificationState,
): VerificationState = if (reconciled is RuntimeResult.Success &&
    current is VerificationState.Verified &&
    !current.snapshot.status.requiresRecovery
) {
    current
} else {
    VerificationState.Unverified
}

internal fun canLaunchAfterSwitch(
    switchResult: RuntimeResult,
    verification: VerificationState,
    packageName: String,
    slotId: String,
): Boolean {
    val switched = (switchResult as? RuntimeResult.Success)?.payload as? RuntimePayload.Switch
        ?: return false
    return switched.packageName == packageName && switched.slotId == slotId &&
        verification is VerificationState.Verified &&
        verification.source == VerificationSource.PostMutation &&
        verification.snapshot.status.packageName == packageName &&
        verification.snapshot.status.activeSlot == slotId &&
        !verification.snapshot.status.requiresRecovery &&
        verification.snapshot.slots.singleOrNull { it.active }?.let { active ->
            active.id == slotId && active.state == "ready"
        } == true
}
