package com.uclone.slices.v2.launcher.hook

import android.app.Application
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.LauncherApps
import android.content.pm.PackageManager
import android.content.pm.ShortcutInfo
import android.graphics.Rect
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Bundle
import android.os.PersistableBundle
import android.os.UserHandle
import android.util.Log
import android.widget.Toast
import com.uclone.slices.v2.launcher.BuildConfig
import com.uclone.slices.v2.launcher.compat.LauncherPackageEvidence
import com.uclone.slices.v2.launcher.compat.isSupported
import com.uclone.slices.v2.launcher.relay.LauncherRelayContract
import io.github.libxposed.api.XposedModule
import io.github.libxposed.api.XposedModuleInterface.ModuleLoadedParam
import io.github.libxposed.api.XposedModuleInterface.PackageReadyParam
import java.lang.reflect.Field
import java.util.concurrent.ConcurrentHashMap

class SlicesLauncherModule : XposedModule() {
    private val injectedPackages = ConcurrentHashMap.newKeySet<String>()

    override fun onModuleLoaded(param: ModuleLoadedParam) {
        log(
            Log.INFO,
            TAG,
            "loaded process=${param.processName} probe=${BuildConfig.HOOK_PROBE_ONLY}",
        )
    }

    override fun onPackageReady(param: PackageReadyParam) {
        if (param.packageName != LauncherRelayContract.LAUNCHER_PACKAGE || !param.isFirstPackage) {
            return
        }
        val application = currentApplication() ?: run {
            log(Log.ERROR, TAG, "compatibility gate failed: application unavailable")
            return
        }
        val evidence = launcherEvidence(application)
        if (evidence?.isSupported() != true) {
            log(
                Log.WARN,
                TAG,
                "compatibility gate refused version=${evidence?.versionCode}/${evidence?.versionName}",
            )
            return
        }
        runCatching {
            installGetShortcutsHook()
            installStartShortcutHooks()
        }.onFailure { error ->
            log(Log.ERROR, TAG, "failed to install LauncherApps seam", error)
        }
    }

    private fun installGetShortcutsHook() {
        val method = LauncherApps::class.java.getDeclaredMethod(
            "getShortcuts",
            LauncherApps.ShortcutQuery::class.java,
            UserHandle::class.java,
        )
        hook(method).intercept { chain ->
            val original = chain.proceed()
            runCatching {
                appendShortcut(
                    context = currentApplication() ?: return@runCatching original,
                    query = chain.args.getOrNull(0),
                    user = chain.args.getOrNull(1) as? UserHandle,
                    original = original,
                )
            }.onFailure { error ->
                log(Log.ERROR, TAG, "getShortcuts adapter failed", error)
            }.getOrDefault(original)
        }
        log(Log.INFO, TAG, "hooked LauncherApps.getShortcuts")
    }

    private fun installStartShortcutHooks() {
        val shortcutMethod = LauncherApps::class.java.getDeclaredMethod(
            "startShortcut",
            ShortcutInfo::class.java,
            Rect::class.java,
            Bundle::class.java,
        )
        hook(shortcutMethod).intercept { chain ->
            val shortcut = chain.args.getOrNull(0) as? ShortcutInfo
            val userId = shortcut?.userHandle.userIdOrUnknown()
            if (
                shortcut == null ||
                !isMarker(shortcut) ||
                !shouldInterceptShortcut(
                    shortcut.`package`,
                    shortcut.id,
                    userId,
                    injectedPackages,
                )
            ) {
                chain.proceed()
            } else {
                handleMarkerClick(shortcut.`package`, userId)
                Unit
            }
        }

        val idMethod = LauncherApps::class.java.getDeclaredMethod(
            "startShortcut",
            String::class.java,
            String::class.java,
            Rect::class.java,
            Bundle::class.java,
            UserHandle::class.java,
        )
        hook(idMethod).intercept { chain ->
            val packageName = chain.args.getOrNull(0) as? String
            val shortcutId = chain.args.getOrNull(1) as? String
            val user = chain.args.getOrNull(4) as? UserHandle
            val userId = user.userIdOrUnknown()
            if (!shouldInterceptShortcut(packageName, shortcutId, userId, injectedPackages)) {
                chain.proceed()
            } else {
                handleMarkerClick(requireNotNull(packageName), userId)
                Unit
            }
        }
        log(Log.INFO, TAG, "hooked LauncherApps.startShortcut overloads")
    }

    private fun appendShortcut(
        context: Context,
        query: Any?,
        user: UserHandle?,
        original: Any?,
    ): Any? {
        val existing = (original as? List<*>)
            .orEmpty()
            .filterIsInstance<ShortcutInfo>()
        val packageName = query.readField<String>("mPackage")
        val userId = user.userIdOrUnknown()
        val key = packageName?.let { shortcutKey(it, userId) }
        val existingMarker = existing.firstOrNull {
            it.id == LauncherRelayContract.MARKER_SHORTCUT_ID
        }
        if (existingMarker != null) {
            key?.let(injectedPackages::remove)
            log(Log.WARN, TAG, "shortcut id collision package=$packageName user=$userId")
            return original
        }

        val evidence = ShortcutQueryEvidence(
            packageName = packageName,
            userId = userId,
            requestedIds = query.readField<List<String>>("mShortcutIds"),
            existingIds = existing.map(ShortcutInfo::getId),
        )
        if (!evidence.canInject() || packageName == null || key == null) {
            key?.let(injectedPackages::remove)
            return original
        }

        val title = if (BuildConfig.HOOK_PROBE_ONLY) {
            if (packageName != LauncherRelayContract.FIXTURE_PACKAGE) {
                injectedPackages -= key
                return original
            }
            LauncherRelayContract.PROBE_TITLE
        } else {
            queryTitle(context, packageName) ?: run {
                injectedPackages -= key
                return original
            }
        }
        val marker = createMarker(context, packageName, title) ?: run {
            injectedPackages -= key
            return original
        }
        injectedPackages += key
        log(Log.INFO, TAG, "getShortcuts inject package=$packageName user=$userId title=$title")
        return ArrayList<ShortcutInfo>(existing.size + 1).apply {
            addAll(existing)
            add(marker)
        }
    }

    private fun createMarker(
        launcherContext: Context,
        packageName: String,
        title: String,
    ): ShortcutInfo? = runCatching {
        val targetContext = launcherContext.createPackageContext(packageName, 0)
        val launchIntent = launcherContext.packageManager
            .getLaunchIntentForPackage(packageName)
            ?: Intent(Intent.ACTION_MAIN).setPackage(packageName)
        val marker = PersistableBundle().apply {
            putString(LauncherRelayContract.MARKER_EXTRA, LauncherRelayContract.MARKER_VALUE)
        }
        val color = ShortcutIconRenderer.themedColor(launcherContext)
        ShortcutInfo.Builder(targetContext, LauncherRelayContract.MARKER_SHORTCUT_ID)
            .setShortLabel(title)
            .setLongLabel(title)
            .setIntent(launchIntent)
            .setExtras(marker)
            .setIcon(Icon.createWithBitmap(ShortcutIconRenderer.render(color)))
            .setRank(0)
            .build()
    }.onFailure { error ->
        log(Log.ERROR, TAG, "could not build marker for $packageName", error)
    }.getOrNull()

    private fun queryTitle(context: Context, packageName: String): String? {
        val bundle = context.contentResolver.call(
            RELAY_URI,
            LauncherRelayContract.METHOD_QUERY_STATE,
            null,
            Bundle().apply {
                putString(LauncherRelayContract.KEY_PACKAGE_NAME, packageName)
            },
        ) ?: return null
        if (!bundle.getBoolean(LauncherRelayContract.KEY_SHOW, false)) return null
        return bundle.getString(LauncherRelayContract.KEY_TARGET_NAME)
            ?.takeIf(String::isNotBlank)
    }

    private fun handleMarkerClick(packageName: String, userId: Int) {
        val context = currentApplication() ?: return
        if (BuildConfig.HOOK_PROBE_ONLY) {
            log(Log.INFO, TAG, "startShortcut probe passed package=$packageName user=$userId")
            Toast.makeText(context, LauncherRelayContract.PROBE_TITLE, Toast.LENGTH_SHORT).show()
            return
        }
        val sent = runCatching {
            val bundle = context.contentResolver.call(
                RELAY_URI,
                LauncherRelayContract.METHOD_CREATE_ACTION,
                null,
                Bundle().apply {
                    putString(LauncherRelayContract.KEY_PACKAGE_NAME, packageName)
                },
            ) ?: return@runCatching false
            if (!bundle.getBoolean(LauncherRelayContract.KEY_SHOW, false)) {
                return@runCatching false
            }
            @Suppress("DEPRECATION")
            val pendingIntent = bundle.getParcelable<PendingIntent>(
                LauncherRelayContract.KEY_PENDING_INTENT,
            ) ?: return@runCatching false
            pendingIntent.send()
            true
        }.onFailure { error ->
            log(Log.ERROR, TAG, "startShortcut adapter failed package=$packageName", error)
        }.getOrDefault(false)
        if (!sent) {
            Toast.makeText(context, "Slices 快捷切换不可用", Toast.LENGTH_SHORT).show()
        }
    }

    private fun isMarker(shortcut: ShortcutInfo): Boolean =
        shortcut.id == LauncherRelayContract.MARKER_SHORTCUT_ID &&
            shortcut.extras?.getString(LauncherRelayContract.MARKER_EXTRA) ==
            LauncherRelayContract.MARKER_VALUE

    private fun currentApplication(): Application? = runCatching {
        Class.forName("android.app.ActivityThread")
            .getDeclaredMethod("currentApplication")
            .invoke(null) as? Application
    }.getOrNull()

    @Suppress("DEPRECATION")
    private fun launcherEvidence(context: Context): LauncherPackageEvidence? = runCatching {
        val info = context.packageManager.getPackageInfo(
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

    private companion object {
        const val TAG = "UCloneSlicesLauncher"
        val RELAY_URI: Uri = Uri.parse("content://${LauncherRelayContract.RELAY_AUTHORITY}")
    }
}

private inline fun <reified T> Any?.readField(name: String): T? {
    if (this == null) return null
    var type: Class<*>? = javaClass
    while (type != null) {
        val field: Field? = runCatching { type.getDeclaredField(name) }.getOrNull()
        if (field != null) {
            field.isAccessible = true
            return field.get(this) as? T
        }
        type = type.superclass
    }
    return null
}

private fun UserHandle?.userIdOrUnknown(): Int {
    if (this == null) return -1
    return runCatching {
        val method = javaClass.getDeclaredMethod("getIdentifier")
        method.isAccessible = true
        method.invoke(this) as Int
    }.getOrDefault(-1)
}
