package com.uclone.slices.v2.gate

import android.content.Intent
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class OriginalLaunchTest {
    @Test fun onlyALaunchOfThePreparedPackageIsReissued() {
        assertTrue(OriginalLaunch.usable("org.example.app", "org.example.app"))
        assertFalse(OriginalLaunch.usable("org.example.other", "org.example.app"))
        assertFalse(OriginalLaunch.usable(null, "org.example.app"))
    }

    @Test fun grantsAndResultForwardingAreNotPassedOn() {
        val original = Intent.FLAG_ACTIVITY_FORWARD_RESULT or Intent.FLAG_GRANT_READ_URI_PERMISSION or
            Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
            Intent.FLAG_GRANT_PREFIX_URI_PERMISSION or Intent.FLAG_ACTIVITY_CLEAR_TOP
        assertEquals(
            Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION,
            OriginalLaunch.flags(original),
        )
        assertEquals(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION, OriginalLaunch.flags(0))
    }
}
