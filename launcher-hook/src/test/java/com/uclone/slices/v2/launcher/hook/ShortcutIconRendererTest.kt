package com.uclone.slices.v2.launcher.hook

import android.content.res.Configuration
import android.graphics.Color
import androidx.test.core.app.ApplicationProvider
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ShortcutIconRendererTest {
    @Test
    fun fallbackColorTracksDayAndNightMode() {
        assertEquals(
            ShortcutIconRenderer.DAY_FALLBACK,
            ShortcutIconRenderer.fallbackColor(Configuration.UI_MODE_NIGHT_NO),
        )
        assertEquals(
            ShortcutIconRenderer.NIGHT_FALLBACK,
            ShortcutIconRenderer.fallbackColor(Configuration.UI_MODE_NIGHT_YES),
        )
    }

    @Test
    fun iconIsTransparentWithTwoThreePixelRoundedLineArrows() {
        val bitmap = ShortcutIconRenderer.render(Color.MAGENTA)

        assertEquals(48, bitmap.width)
        assertEquals(48, bitmap.height)
        assertEquals(Color.TRANSPARENT, bitmap.getPixel(0, 0))
        assertEquals(Color.MAGENTA, bitmap.getPixel(24, 17))
        assertEquals(Color.MAGENTA, bitmap.getPixel(24, 31))
        assertTrue(Color.alpha(bitmap.getPixel(24, 24)) == 0)
    }

    @Test
    fun themedColorAlwaysResolvesToAnOpaqueForeground() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()

        assertEquals(255, Color.alpha(ShortcutIconRenderer.themedColor(context)))
    }
}
