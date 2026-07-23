package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.payloadAs

data class PackageSnapshot(
    val status: PackageRuntimeStatus,
    val slots: List<SlotSpace>,
)

suspend fun RuntimeGateway.readPackageSnapshot(packageName: String): PackageSnapshot? {
    return validatePackageSnapshot(packageName, packageSnapshot(packageName))
}

internal fun validatePackageSnapshot(
    packageName: String,
    result: RuntimeResult,
): PackageSnapshot? {
    val snapshot = result.payloadAs<RuntimePayload.PackageSnapshot>() ?: return null
    if (snapshot.status.packageName != packageName) return null
    if (snapshot.slots.map { it.id }.distinct().size != snapshot.slots.size) return null
    if (snapshot.slots.count { it.active } != 1) return null
    val active = snapshot.slots.single { it.active }
    if (active.id != snapshot.status.activeSlot || active.state != "ready") return null
    return PackageSnapshot(snapshot.status, snapshot.slots)
}
