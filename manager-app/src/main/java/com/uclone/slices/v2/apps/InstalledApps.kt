package com.uclone.slices.v2.apps

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
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
                val packageInfo = packageManager.getPackageInfo(application.packageName, 0)
                InstalledApp(
                    packageName = application.packageName,
                    label = packageManager.getApplicationLabel(application).toString(),
                    icon = packageManager.getApplicationIcon(application),
                    signingIdentity = signingIdentity(application.packageName),
                    versionName = packageInfo.versionName.orEmpty(),
                    versionCode = packageInfo.longVersionCode,
                )
            }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.label })
            .toList()
    }

    private fun signingIdentity(packageName: String): SigningIdentity? = runCatching {
        val info = packageManager.getPackageInfo(
            packageName,
            PackageManager.GET_SIGNING_CERTIFICATES,
        ).signingInfo ?: return@runCatching null
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
