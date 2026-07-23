package com.uclone.slots.preview

import com.uclone.slots.preview.model.InstalledApp
import com.uclone.slots.preview.model.ManagedApp
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.RuntimeHealth
import com.uclone.slots.preview.model.RuntimeMode
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class PackageIsolationPolicyTest {
    @Test
    fun recoveryOnlyModeComesOnlyFromExplicitProbeField() {
        assertEquals(RuntimeMode.RecoveryOnly, RuntimeMode.fromProbe(true))
        assertEquals(RuntimeMode.Ordinary, RuntimeMode.fromProbe(false))
        assertEquals(RuntimeHealth.RecoveryRequired, RuntimeHealth.fromProbe(true, false, false, true))
        assertEquals(RuntimeHealth.UserLocked, RuntimeHealth.fromProbe(false, false, false, true))
    }

    @Test
    fun brokenPackageAStillAllowsHealthyPackageBToBeQueried() = runBlocking {
        val queried = mutableListOf<String>()
        val statuses = mapOf(
            "com.example.a" to status("com.example.a", PackageLifecycle.RecoveryRequired),
            "com.example.b" to status("com.example.b", PackageLifecycle.Normal),
        )
        suspend fun query(packageName: String) = queryPackageForMode(
            RuntimeMode.Ordinary,
            packageName,
        ) {
            queried += it
            statuses.getValue(it)
        }

        assertEquals(PackageLifecycle.RecoveryRequired, query("com.example.a")?.lifecycle)
        assertEquals(PackageLifecycle.Normal, query("com.example.b")?.lifecycle)
        assertEquals(listOf("com.example.a", "com.example.b"), queried)
    }

    @Test
    fun packageFailureMarksOnlyThatPackageAndSelectedStatus() {
        val apps = listOf(
            ManagedApp("com.example.a", "A", "base", PackageLifecycle.Normal),
            ManagedApp("com.example.b", "B", "base", PackageLifecycle.Normal),
        ).withPackageLifecycle("com.example.a", PackageLifecycle.RecoveryRequired)
        val selected = status("com.example.a", PackageLifecycle.Normal)
            .withPackageLifecycle("com.example.a", PackageLifecycle.RecoveryRequired)

        assertEquals(PackageLifecycle.RecoveryRequired, apps[0].lifecycle)
        assertEquals(PackageLifecycle.Normal, apps[1].lifecycle)
        assertEquals(PackageLifecycle.RecoveryRequired, selected?.lifecycle)
    }

    @Test
    fun recoveryOnlyModeSkipsPackageInspectionAndExposesOnlyBaseRescue() = runBlocking {
        var queried = false

        val result = queryPackageForMode(RuntimeMode.RecoveryOnly, "com.example.a") {
            queried = true
            status(it, PackageLifecycle.Normal)
        }

        assertNull(result)
        assertFalse(queried)
        assertFalse(RuntimeMode.RecoveryOnly.allowsReconcile)
        assertTrue(RuntimeMode.RecoveryOnly.allowsBaseRescue)
    }

    @Test
    fun coldCacheRecoverySelectionAddsOnlyTheChosenInstalledApp() {
        val apps = listOf(
            ManagedApp("com.example.peer", "Peer", "base", PackageLifecycle.Normal),
        ).withRecoveryTarget(InstalledApp("com.example.rescue", "Rescue"))

        assertEquals("com.example.rescue", apps.first().packageName)
        assertEquals("unknown", apps.first().activeSlot)
        assertEquals(PackageLifecycle.RecoveryRequired, apps.first().lifecycle)
        assertEquals(PackageLifecycle.Normal, apps.last().lifecycle)
    }

    @Test
    fun successfulRescueRemovesOnlyTheRetiredPackageRow() {
        val apps = listOf(
            ManagedApp("com.example.retired", "Retired", "base", PackageLifecycle.Normal),
            ManagedApp("com.example.kept", "Kept", "work", PackageLifecycle.Normal),
        ).withoutPackage("com.example.retired")

        assertEquals(listOf("com.example.kept"), apps.map { it.packageName })
    }

    private fun status(packageName: String, lifecycle: PackageLifecycle) = PackageRuntimeStatus(
        packageName = packageName,
        activeSlot = "base",
        lifecycle = lifecycle,
        enabled = lifecycle == PackageLifecycle.Normal,
        suspended = false,
    )
}
