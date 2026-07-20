package com.uclone.slots.preview.apps

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import com.uclone.slots.preview.model.InstalledApp

class InstalledAppsRepository(private val context: Context) {
    fun launcherApps(): List<InstalledApp> {
        val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_ALL)
            .asSequence()
            .mapNotNull { it.activityInfo?.applicationInfo }
            .filter { it.packageName != context.packageName }
            .filterNot(::isSystem)
            .distinctBy { it.packageName }
            .map { info ->
                InstalledApp(
                    packageName = info.packageName,
                    label = info.loadLabel(context.packageManager).toString(),
                )
            }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
            .toList()
    }

    fun label(packageName: String): String = try {
        val info = context.packageManager.getApplicationInfo(packageName, 0)
        info.loadLabel(context.packageManager).toString()
    } catch (_: PackageManager.NameNotFoundException) {
        packageName
    }

    private fun isSystem(info: ApplicationInfo): Boolean {
        val mask = ApplicationInfo.FLAG_SYSTEM or ApplicationInfo.FLAG_UPDATED_SYSTEM_APP
        return info.flags and mask != 0
    }
}
