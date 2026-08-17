package com.uclone.slices.v2.launcher.hook

import com.uclone.slices.v2.launcher.relay.LauncherRelayContract
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ShortcutInjectionPolicyTest {
    @Test
    fun injectsOnlyAUserZeroPackageQueryWithoutAnExistingMarker() {
        val valid = ShortcutQueryEvidence(
            packageName = "com.tencent.mm",
            userId = 0,
            requestedIds = null,
            existingIds = listOf("scan"),
        )

        assertTrue(valid.canInject())
        assertFalse(valid.copy(packageName = null).canInject())
        assertFalse(valid.copy(userId = 10).canInject())
        assertFalse(
            valid.copy(requestedIds = listOf("another_shortcut")).canInject(),
        )
        assertFalse(
            valid.copy(
                existingIds = listOf(LauncherRelayContract.MARKER_SHORTCUT_ID),
            ).canInject(),
        )
    }

    @Test
    fun interceptsOnlyTheMarkerForAPackageThatWasActuallyInjected() {
        val injected = setOf(shortcutKey("com.tencent.mm", 0))

        assertTrue(
            shouldInterceptShortcut(
                "com.tencent.mm",
                LauncherRelayContract.MARKER_SHORTCUT_ID,
                0,
                injected,
            ),
        )
        assertFalse(shouldInterceptShortcut("com.tencent.mm", "scan", 0, injected))
        assertFalse(
            shouldInterceptShortcut(
                "com.example.other",
                LauncherRelayContract.MARKER_SHORTCUT_ID,
                0,
                injected,
            ),
        )
        assertFalse(
            shouldInterceptShortcut(
                "com.tencent.mm",
                LauncherRelayContract.MARKER_SHORTCUT_ID,
                10,
                injected,
            ),
        )
    }
}
