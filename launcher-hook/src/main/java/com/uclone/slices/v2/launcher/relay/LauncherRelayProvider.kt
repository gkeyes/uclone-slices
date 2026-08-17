package com.uclone.slices.v2.launcher.relay

import android.app.PendingIntent
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Binder
import android.os.Bundle
import com.uclone.slices.v2.launcher.compat.LauncherPackageEvidence
import com.uclone.slices.v2.launcher.compat.isAllowedLauncherCaller
import java.util.UUID

class LauncherRelayProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        if (!allowedCaller()) return hidden()
        val callingIdentity = Binder.clearCallingIdentity()
        return try {
            val context = requireNotNull(context)
            val packageName = extras
                ?.getString(LauncherRelayContract.KEY_PACKAGE_NAME)
                ?.takeIf(String::isNotBlank)
                ?: return hidden()
            val managerState = queryManagerState(packageName) ?: return hidden()
            if (!managerState.getBoolean(LauncherRelayContract.KEY_SHOW, false)) return hidden()
            when (method) {
                LauncherRelayContract.METHOD_QUERY_STATE -> managerState
                LauncherRelayContract.METHOD_CREATE_ACTION -> managerState.apply {
                    val requestId = UUID.randomUUID().toString()
                    putString(LauncherRelayContract.KEY_REQUEST_ID, requestId)
                    putParcelable(
                        LauncherRelayContract.KEY_PENDING_INTENT,
                        createActionToken(context, packageName, requestId),
                    )
                }
                else -> hidden()
            }
        } finally {
            Binder.restoreCallingIdentity(callingIdentity)
        }
    }

    private fun allowedCaller(): Boolean {
        val context = requireNotNull(context)
        val packageManager = context.packageManager
        val callerPackages = packageManager
            .getPackagesForUid(Binder.getCallingUid())
            .orEmpty()
            .toSet()
        @Suppress("DEPRECATION")
        val evidence = runCatching {
            val info = packageManager.getPackageInfo(
                LauncherRelayContract.LAUNCHER_PACKAGE,
                0,
            )
            LauncherPackageEvidence(
                packageName = info.packageName,
                versionCode = info.longVersionCode,
                versionName = info.versionName,
                applicationFlags = requireNotNull(info.applicationInfo).flags,
            )
        }.getOrNull()
        return isAllowedLauncherCaller(callerPackages, evidence)
    }

    private fun queryManagerState(packageName: String): Bundle? = runCatching {
        requireNotNull(context).contentResolver.call(
            Uri.parse("content://${LauncherRelayContract.MANAGER_STATE_AUTHORITY}"),
            LauncherRelayContract.METHOD_QUERY_STATE,
            null,
            Bundle().apply {
                putString(LauncherRelayContract.KEY_PACKAGE_NAME, packageName)
            },
        )
    }.getOrNull()

    private fun hidden(): Bundle = Bundle().apply {
        putBoolean(LauncherRelayContract.KEY_SHOW, false)
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0
    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0
}

internal fun createActionToken(
    context: android.content.Context,
    packageName: String,
    requestId: String,
): PendingIntent {
    val actionIntent = Intent(LauncherRelayContract.MANAGER_ACTION)
        .setClassName(
            LauncherRelayContract.MANAGER_PACKAGE,
            LauncherRelayContract.MANAGER_SERVICE,
        )
        .setData(
            Uri.Builder()
                .scheme("uclone-slices")
                .authority("desktop-switch")
                .appendPath(requestId)
                .build(),
        )
        .putExtra(LauncherRelayContract.KEY_PACKAGE_NAME, packageName)
        .putExtra(LauncherRelayContract.KEY_REQUEST_ID, requestId)
    return PendingIntent.getForegroundService(
        context,
        requestId.hashCode(),
        actionIntent,
        PendingIntent.FLAG_ONE_SHOT or PendingIntent.FLAG_IMMUTABLE,
    )
}
