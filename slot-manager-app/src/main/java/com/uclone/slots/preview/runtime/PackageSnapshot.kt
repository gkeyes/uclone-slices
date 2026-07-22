package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.payloadAs

data class PackageSnapshot(
    val status: PackageRuntimeStatus,
    val slots: List<SlotSpace>,
)

suspend fun RuntimeGateway.readPackageSnapshot(packageName: String): PackageSnapshot? {
    return validatePackageSnapshot(packageName, status(packageName), listSlots(packageName))
}

internal fun validatePackageSnapshot(
    packageName: String,
    statusResult: RuntimeResult,
    slotsResult: RuntimeResult,
): PackageSnapshot? {
    val status = statusResult.payloadAs<RuntimePayload.Status>()?.value ?: return null
    val slots = slotsResult.payloadAs<RuntimePayload.Slots>() ?: return null
    if (status.packageName != packageName || slots.packageName != packageName) return null
    if (slots.rows.map { it.id }.distinct().size != slots.rows.size) return null
    if (slots.rows.none { it.id == status.activeSlot && it.active && it.state == "ready" }) return null
    if (slots.rows.count { it.active } != 1) return null
    return PackageSnapshot(status, slots.rows)
}
