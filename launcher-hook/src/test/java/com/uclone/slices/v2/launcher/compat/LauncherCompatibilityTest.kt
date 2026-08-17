package com.uclone.slices.v2.launcher.compat

import android.content.pm.ApplicationInfo
import com.uclone.slices.v2.launcher.relay.LauncherRelayContract
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class LauncherCompatibilityTest {
    private val supported = LauncherPackageEvidence(
        packageName = LauncherRelayContract.LAUNCHER_PACKAGE,
        versionCode = LauncherRelayContract.LAUNCHER_VERSION_CODE,
        versionName = LauncherRelayContract.LAUNCHER_VERSION_NAME,
        applicationFlags = ApplicationInfo.FLAG_UPDATED_SYSTEM_APP,
    )

    @Test
    fun exactSystemLauncherIsAccepted() {
        assertTrue(supported.isSupported())
        assertTrue(
            isAllowedLauncherCaller(
                setOf(LauncherRelayContract.LAUNCHER_PACKAGE),
                supported,
            ),
        )
    }

    @Test
    fun unknownVersionThirdPartyLauncherAndWrongCallerFailClosed() {
        assertFalse(supported.copy(versionCode = supported.versionCode + 1).isSupported())
        assertFalse(supported.copy(versionName = "future").isSupported())
        assertFalse(supported.copy(applicationFlags = 0).isSupported())
        assertFalse(isAllowedLauncherCaller(setOf("com.example.launcher"), supported))
        assertFalse(
            isAllowedLauncherCaller(
                setOf(LauncherRelayContract.LAUNCHER_PACKAGE),
                null,
            ),
        )
    }
}
