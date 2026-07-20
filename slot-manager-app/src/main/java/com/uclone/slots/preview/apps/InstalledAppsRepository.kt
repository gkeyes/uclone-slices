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

    fun rememberManaged(packageNames: Collection<String>) {
        val safe = packageNames.asSequence().filter(::isSafePackage).take(MAX_MANAGED).toSet()
        preferences.edit().putStringSet(MANAGED_PACKAGES, safe).apply()
    }

    fun cachedManaged(): List<String> = preferences
        .getStringSet(MANAGED_PACKAGES, emptySet())
        .orEmpty()
        .asSequence()
        .filter(::isSafePackage)
        .take(MAX_MANAGED)
        .sorted()
        .toList()

    private fun isSystem(info: ApplicationInfo): Boolean {
        val mask = ApplicationInfo.FLAG_SYSTEM or ApplicationInfo.FLAG_UPDATED_SYSTEM_APP
        return info.flags and mask != 0
    }

    private fun isSafePackage(value: String): Boolean =
        value.length <= 255 && PACKAGE_PATTERN.matches(value)

    private val preferences
        get() = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private companion object {
        const val PREFERENCES = "managed-app-cache"
        const val MANAGED_PACKAGES = "packages"
        const val MAX_MANAGED = 128
        val PACKAGE_PATTERN = Regex("[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+")
    }
}
