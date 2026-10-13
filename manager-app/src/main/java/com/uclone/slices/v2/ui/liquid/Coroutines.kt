/*
 * Vendored from Kyant0/AndroidLiquidGlass (https://github.com/Kyant0/AndroidLiquidGlass),
 * catalog app @ 65ab177, Copyright Kyant, licensed under the Apache License 2.0
 * (http://www.apache.org/licenses/LICENSE-2.0). Package renamed; ported via DualSimRouting
 * (gkeyes/dualsimrouting, ui/ios27-liquid-glass).
 */
package com.uclone.slices.v2.ui.liquid

internal suspend fun awaitFrame() {
    kotlinx.coroutines.android.awaitFrame()
}
