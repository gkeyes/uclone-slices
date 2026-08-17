package com.uclone.slices.v2.launcher.relay

import android.app.PendingIntent
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ActionTokenTest {
    @Test
    fun actionTokenIsExplicitImmutableOneShotAndRequestUnique() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val first = createActionToken(context, "com.tencent.mm", "request-one")
        val second = createActionToken(context, "com.tencent.mm", "request-two")
        val shadow = shadowOf(first)
        val intent = shadow.savedIntent

        assertTrue(shadow.flags and PendingIntent.FLAG_ONE_SHOT != 0)
        assertTrue(shadow.flags and PendingIntent.FLAG_IMMUTABLE != 0)
        assertEquals(LauncherRelayContract.MANAGER_PACKAGE, intent.component?.packageName)
        assertEquals(LauncherRelayContract.MANAGER_SERVICE, intent.component?.className)
        assertEquals("com.tencent.mm", intent.getStringExtra(LauncherRelayContract.KEY_PACKAGE_NAME))
        assertEquals("request-one", intent.getStringExtra(LauncherRelayContract.KEY_REQUEST_ID))
        assertEquals("request-one", intent.data?.lastPathSegment)
        assertNotEquals(first, second)
    }
}
