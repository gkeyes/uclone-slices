package com.uclone.slices.v2.ui

import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SlotSnapshot

internal data class AppSections(
    val managed: List<InstalledApp>,
    val other: List<InstalledApp>,
)

internal fun buildAppSections(
    installedApps: List<InstalledApp>,
    packages: List<PackageSnapshot>,
    query: String,
): AppSections {
    val normalizedQuery = query.trim()
    val visibleApps = if (normalizedQuery.isEmpty()) {
        installedApps
    } else {
        installedApps.filter { app ->
            app.label.contains(normalizedQuery, ignoreCase = true) ||
                app.packageName.contains(normalizedQuery, ignoreCase = true)
        }
    }
    val managedPackages = packages.asSequence().map(PackageSnapshot::packageName).toSet()
    return AppSections(
        managed = visibleApps.filter { it.packageName in managedPackages },
        other = visibleApps.filterNot { it.packageName in managedPackages },
    )
}

internal fun activeSlotDisplayName(packageSnapshot: PackageSnapshot): String =
    packageSnapshot.slots
        .firstOrNull { it.id == packageSnapshot.activeSlot }
        ?.let(::spaceDisplayName)
        ?: packageSnapshot.activeSlot

internal fun spaceDisplayName(slot: SlotSnapshot): String =
    if (slot.id == BASE_SLOT_ID) {
        "系统原始空间"
    } else {
        slot.name.takeIf(String::isNotBlank) ?: slot.id
    }

internal fun independentSpaceCount(packageSnapshot: PackageSnapshot): Int =
    packageSnapshot.slots.count { it.id != BASE_SLOT_ID }

internal fun quickSwitchSlots(packageSnapshot: PackageSnapshot): List<SlotSnapshot> {
    val (base, ordinary) = packageSnapshot.slots.partition { it.id == BASE_SLOT_ID }
    return base + ordinary
}
