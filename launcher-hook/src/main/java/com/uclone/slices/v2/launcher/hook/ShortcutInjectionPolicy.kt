package com.uclone.slices.v2.launcher.hook

import com.uclone.slices.v2.launcher.relay.LauncherRelayContract

internal data class ShortcutQueryEvidence(
    val packageName: String?,
    val userId: Int,
    val requestedIds: List<String>?,
    val existingIds: List<String>,
)

internal fun ShortcutQueryEvidence.canInject(): Boolean =
    !packageName.isNullOrBlank() &&
        userId == 0 &&
        (requestedIds.isNullOrEmpty() ||
            LauncherRelayContract.MARKER_SHORTCUT_ID in requestedIds) &&
        LauncherRelayContract.MARKER_SHORTCUT_ID !in existingIds

internal fun shortcutKey(packageName: String, userId: Int): String = "$userId:$packageName"

internal fun shouldInterceptShortcut(
    packageName: String?,
    shortcutId: String?,
    userId: Int,
    injectedPackages: Set<String>,
): Boolean =
    packageName != null &&
        shortcutId == LauncherRelayContract.MARKER_SHORTCUT_ID &&
        shortcutKey(packageName, userId) in injectedPackages
