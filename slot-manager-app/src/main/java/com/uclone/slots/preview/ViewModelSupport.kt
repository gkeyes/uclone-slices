package com.uclone.slots.preview

import android.app.Application
import android.content.Intent
import com.uclone.slots.preview.model.RuntimeHealth
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult

internal object UiMappings {
    fun probeHealth(payload: RuntimePayload): RuntimeHealth {
        val probe = payload as? RuntimePayload.Probe ?: return RuntimeHealth.Unsupported
        return when {
            !probe.userUnlocked -> RuntimeHealth.UserLocked
            probe.ready && probe.ceDeSupported -> RuntimeHealth.Ready
            else -> RuntimeHealth.Unsupported
        }
    }

    fun healthForError(code: String) = when (code) {
        "user_locked" -> RuntimeHealth.UserLocked
        "recovery_required", "quarantined" -> RuntimeHealth.RecoveryRequired
        "unsupported_device" -> RuntimeHealth.Unsupported
        else -> RuntimeHealth.DaemonOffline
    }

    fun errorText(code: String) = when (code) {
        "busy" -> "另一项数据空间操作正在进行"
        "conflict" -> "当前状态与该操作冲突，请先刷新状态"
        "package_not_allowed" -> "该应用不在 Preview 支持范围"
        "direct_boot_confirmation_required" -> "请确认 Direct Boot 条件支持提示后再登记"
        "not_found" -> "目标尚未登记或空间不存在"
        "user_locked" -> "请先解锁主用户"
        else -> "Runtime 拒绝操作：$code"
    }
}

internal inline fun <reified T : RuntimePayload> RuntimeResult.payloadAs(): T? =
    (this as? RuntimeResult.Success)?.payload as? T

internal fun launchInstalledApp(application: Application, packageName: String): Boolean {
    val intent = application.packageManager
        .getLaunchIntentForPackage(packageName)
        ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ?: return false
    application.startActivity(intent)
    return true
}
