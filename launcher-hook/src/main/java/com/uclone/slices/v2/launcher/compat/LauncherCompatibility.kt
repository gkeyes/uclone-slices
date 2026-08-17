package com.uclone.slices.v2.launcher.compat

import android.content.pm.ApplicationInfo
import com.uclone.slices.v2.launcher.relay.LauncherRelayContract

internal data class LauncherPackageEvidence(
    val packageName: String,
    val versionCode: Long,
    val versionName: String?,
    val applicationFlags: Int,
)

internal fun LauncherPackageEvidence.isSupported(): Boolean {
    val isSystem = applicationFlags and ApplicationInfo.FLAG_SYSTEM != 0 ||
        applicationFlags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP != 0
    return packageName == LauncherRelayContract.LAUNCHER_PACKAGE &&
        versionCode == LauncherRelayContract.LAUNCHER_VERSION_CODE &&
        versionName == LauncherRelayContract.LAUNCHER_VERSION_NAME &&
        isSystem
}

internal fun isAllowedLauncherCaller(
    callerPackages: Set<String>,
    evidence: LauncherPackageEvidence?,
): Boolean =
    LauncherRelayContract.LAUNCHER_PACKAGE in callerPackages && evidence?.isSupported() == true
