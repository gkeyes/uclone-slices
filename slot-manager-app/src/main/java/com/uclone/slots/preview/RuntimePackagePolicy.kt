package com.uclone.slots.preview

import com.uclone.slots.preview.model.InstalledApp
import com.uclone.slots.preview.model.ManagedApp
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.RuntimeMode

internal suspend fun <T> queryPackageForMode(
    mode: RuntimeMode,
    packageName: String,
    query: suspend (String) -> T,
): T? = if (mode == RuntimeMode.RecoveryOnly) null else query(packageName)

internal fun List<ManagedApp>.withPackageLifecycle(
    packageName: String,
    lifecycle: PackageLifecycle,
): List<ManagedApp> = map {
    if (it.packageName == packageName) it.copy(lifecycle = lifecycle) else it
}

internal fun PackageRuntimeStatus?.withPackageLifecycle(
    packageName: String,
    lifecycle: PackageLifecycle,
): PackageRuntimeStatus? = this?.let {
    if (it.packageName == packageName) it.copy(lifecycle = lifecycle) else it
}

internal fun List<ManagedApp>.withRecoveryTarget(app: InstalledApp): List<ManagedApp> =
    listOf(
        ManagedApp(app.packageName, app.label, "unknown", PackageLifecycle.RecoveryRequired),
    ) + filterNot { it.packageName == app.packageName }

internal fun List<ManagedApp>.withoutPackage(packageName: String): List<ManagedApp> =
    filterNot { it.packageName == packageName }
