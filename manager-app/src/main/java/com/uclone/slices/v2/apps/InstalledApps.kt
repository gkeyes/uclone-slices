package com.uclone.slices.v2.apps

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.graphics.drawable.Drawable
import android.os.Process
import android.os.UserHandle

data class InstalledApp(
    val packageName: String,
    val label: String,
    val icon: Drawable? = null,
)

fun interface InstalledAppsSource {
    fun load(): List<InstalledApp>
}

class PackageManagerInstalledApps(
    context: Context,
) : InstalledAppsSource {
    private val packageManager = context.packageManager
    private val ownPackage = context.packageName

    override fun load(): List<InstalledApp> {
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return packageManager
            .queryIntentActivities(launcher, 0)
            .asSequence()
            .map { it.activityInfo.applicationInfo }
            .filter { application ->
                application.packageName != ownPackage &&
                    application.flags and ApplicationInfo.FLAG_SYSTEM == 0 &&
                    UserHandle.getUserHandleForUid(application.uid) == Process.myUserHandle()
            }
            .distinctBy(ApplicationInfo::packageName)
            .map { application ->
                InstalledApp(
                    packageName = application.packageName,
                    label = packageManager.getApplicationLabel(application).toString(),
                    icon = packageManager.getApplicationIcon(application),
                )
            }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
            .toList()
    }
}
