package com.uclone.slices.v2.apps

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.graphics.drawable.Drawable
import android.os.Process
import android.os.UserHandle
import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import java.security.MessageDigest

data class InstalledApp(
    val packageName: String,
    val label: String,
    val icon: Drawable? = null,
    val signingIdentity: SigningIdentity? = null,
    val versionName: String = "",
    val versionCode: Long = 0,
)

fun interface InstalledAppsSource {
    fun load(): List<InstalledApp>
}

interface OptimizedInstalledAppsSource {
    fun loadPackages(packageNames: Set<String>): List<InstalledApp>
    fun loadCatalog(): List<InstalledApp>
}

internal fun InstalledAppsSource.loadPackagesOptimized(
    packageNames: Set<String>,
): List<InstalledApp> {
    if (packageNames.isEmpty()) return emptyList()
    return (this as? OptimizedInstalledAppsSource)?.loadPackages(packageNames)
        ?: load().filter { it.packageName in packageNames }
}

internal fun InstalledAppsSource.loadCatalogOptimized(): List<InstalledApp> =
    (this as? OptimizedInstalledAppsSource)?.loadCatalog() ?: load()

internal fun InstalledAppsSource.loadAppOptimized(packageName: String): InstalledApp? =
    loadPackagesOptimized(setOf(packageName)).firstOrNull()

class PackageManagerInstalledApps(
    context: Context,
) : InstalledAppsSource, OptimizedInstalledAppsSource {
    private val packageManager = context.packageManager
    private val ownPackage = context.packageName

    override fun load(): List<InstalledApp> =
        launcherApplications()
            .mapNotNull(::loadFullApp)
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })

    override fun loadCatalog(): List<InstalledApp> =
        launcherApplications()
            .map(::loadCatalogApp)
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })

    override fun loadPackages(packageNames: Set<String>): List<InstalledApp> =
        packageNames
            .asSequence()
            .mapNotNull { packageName ->
                runCatching {
                    packageManager.getApplicationInfo(packageName, 0)
                }.getOrNull()
            }
            .filter(::isEligible)
            .mapNotNull(::loadFullApp)
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
            .toList()

    private fun launcherApplications(): List<ApplicationInfo> {
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return packageManager
            .queryIntentActivities(launcher, 0)
            .asSequence()
            .map { it.activityInfo.applicationInfo }
            .filter(::isEligible)
            .distinctBy(ApplicationInfo::packageName)
            .toList()
    }

    private fun isEligible(application: ApplicationInfo): Boolean =
        application.packageName != ownPackage &&
            application.flags and ApplicationInfo.FLAG_SYSTEM == 0 &&
            UserHandle.getUserHandleForUid(application.uid) == Process.myUserHandle()

    private fun loadCatalogApp(application: ApplicationInfo): InstalledApp =
        InstalledApp(
            packageName = application.packageName,
            label = packageManager.getApplicationLabel(application).toString(),
            icon = runCatching { packageManager.getApplicationIcon(application) }.getOrNull(),
        )

    private fun loadFullApp(application: ApplicationInfo): InstalledApp? = runCatching {
        val packageInfo = packageManager.getPackageInfo(
            application.packageName,
            PackageManager.GET_SIGNING_CERTIFICATES,
        )
        InstalledApp(
            packageName = application.packageName,
            label = packageManager.getApplicationLabel(application).toString(),
            icon = runCatching { packageManager.getApplicationIcon(application) }.getOrNull(),
            signingIdentity = signingIdentity(packageInfo),
            versionName = packageInfo.versionName.orEmpty(),
            versionCode = packageInfo.longVersionCode,
        )
    }.getOrNull()

    private fun signingIdentity(packageInfo: PackageInfo): SigningIdentity? = runCatching {
        val info = packageInfo.signingInfo ?: return@runCatching null
        val multiple = info.hasMultipleSigners()
        val signatures = if (multiple) {
            info.apkContentsSigners.orEmpty().toList()
        } else {
            info.signingCertificateHistory.orEmpty().toList()
        }
        signingIdentityFromCertificates(
            multiple = multiple,
            certificates = signatures.map { it.toByteArray() },
        )
    }.getOrNull()
}

internal fun signingIdentityFromCertificates(
    multiple: Boolean,
    certificates: List<ByteArray>,
): SigningIdentity? {
    val digests = certificates
        .map { certificate ->
            MessageDigest.getInstance("SHA-256")
                .digest(certificate)
                .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
        }
        .let { values -> if (multiple) values.sorted() else values }
    return SigningIdentity(
        kind = if (multiple) SigningKind.Multiple else SigningKind.Lineage,
        sha256 = digests,
    ).takeIf { it.isValid() }
}
