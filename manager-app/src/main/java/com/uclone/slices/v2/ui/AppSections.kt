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

internal fun activeSlotDisplayName(packageSnapshot: PackageSnapshot, baseName: String): String =
    packageSnapshot.slots
        .firstOrNull { it.id == packageSnapshot.activeSlot }
        ?.let { spaceDisplayName(it, baseName) }
        ?: packageSnapshot.activeSlot

/** [baseName] is the localised name of the system original space (`system_original_space`). */
internal fun spaceDisplayName(slot: SlotSnapshot, baseName: String): String =
    if (slot.id == BASE_SLOT_ID) {
        baseName
    } else {
        slot.name.takeIf(String::isNotBlank) ?: slot.id
    }

internal fun independentSpaceCount(packageSnapshot: PackageSnapshot): Int =
    packageSnapshot.slots.count { it.id != BASE_SLOT_ID }

internal fun quickSwitchSlots(packageSnapshot: PackageSnapshot): List<SlotSnapshot> {
    val (base, ordinary) = packageSnapshot.slots.partition { it.id == BASE_SLOT_ID }
    return base + ordinary
}
