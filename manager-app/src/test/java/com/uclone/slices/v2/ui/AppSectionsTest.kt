package com.uclone.slices.v2.ui

import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class AppSectionsTest {
    private val apps = listOf(
        InstalledApp("com.alpha", "Alpha"),
        InstalledApp("com.beta", "Beta"),
        InstalledApp("com.gamma", "Gamma"),
    )

    @Test
    fun configuredAppsArePartitionedFirstWithoutChangingSourceOrder() {
        val sections = buildAppSections(
            installedApps = apps,
            packages = listOf(packageSnapshot("com.beta"), packageSnapshot("com.alpha")),
            query = "",
        )

        assertEquals(listOf("com.alpha", "com.beta"), sections.managed.map { it.packageName })
        assertEquals(listOf("com.gamma"), sections.other.map { it.packageName })
    }

    @Test
    fun searchMatchesLabelsAndPackageNamesIgnoringCase() {
        assertEquals(
            listOf("com.beta"),
            buildAppSections(apps, emptyList(), "BETA").other.map { it.packageName },
        )
        assertEquals(
            listOf("com.gamma"),
            buildAppSections(apps, emptyList(), "GAMM").other.map { it.packageName },
        )
    }

    @Test
    fun emptySearchResultProducesNoGroup() {
        val sections = buildAppSections(apps, listOf(packageSnapshot("com.alpha")), "missing")

        assertTrue(sections.managed.isEmpty())
        assertTrue(sections.other.isEmpty())
    }

    @Test
    fun activeSlotUsesDisplayNameAndFallsBackToId() {
        assertEquals("Work", activeSlotDisplayName(packageSnapshot("com.alpha")))
        assertEquals(
            "系统原始空间",
            activeSlotDisplayName(packageSnapshot("com.alpha").copy(activeSlot = "base")),
        )
        assertEquals(
            "slot-missing",
            activeSlotDisplayName(
                packageSnapshot("com.alpha").copy(activeSlot = "slot-missing"),
            ),
        )
        assertEquals(1, independentSpaceCount(packageSnapshot("com.alpha")))
    }

    @Test
    fun quickSwitchSlotsAlwaysPutBaseFirstAndPreserveOrdinaryOrder() {
        val snapshot = packageSnapshot("com.alpha").copy(
            slots = listOf(
                SlotSnapshot("slot-2", "Personal"),
                SlotSnapshot("base", "Base"),
                SlotSnapshot("slot-1", "Work"),
            ),
        )

        assertEquals(
            listOf("base", "slot-2", "slot-1"),
            quickSwitchSlots(snapshot).map(SlotSnapshot::id),
        )
    }
}

private fun packageSnapshot(packageName: String) = PackageSnapshot(
    packageName = packageName,
    activeSlot = "slot-1",
    slots = listOf(
        SlotSnapshot("base", "Base"),
        SlotSnapshot("slot-1", "Work"),
    ),
)
