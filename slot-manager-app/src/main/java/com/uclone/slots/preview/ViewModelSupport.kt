package com.uclone.slots.preview

import android.content.Context
import android.content.Intent
import android.content.ActivityNotFoundException
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
        "runtime_pair_mismatch" -> RuntimeHealth.ModuleMissing
        else -> RuntimeHealth.DaemonOffline
    }

    fun errorText(code: String) = when (code) {
        "busy" -> "另一项数据空间操作正在进行"
        "conflict" -> "当前状态与该操作冲突，请先刷新状态"
        "package_not_allowed" -> "该应用不在 Preview 支持范围"
        "direct_boot_confirmation_required" -> "请确认 Direct Boot 条件支持提示后再登记"
        "not_found" -> "目标尚未登记或空间不存在"
        "user_locked" -> "请先解锁主用户"
        "runtime_pair_mismatch" -> "APK 与 Runtime 不是同一构建，请成对更新"
        else -> "Runtime 拒绝操作：$code"
    }
}

internal inline fun <reified T : RuntimePayload> RuntimeResult.payloadAs(): T? =
    (this as? RuntimeResult.Success)?.payload as? T

internal fun SlotsViewModel.handleInspectionFailure(result: RuntimeResult) {
    when (result) {
        is RuntimeResult.Rejected -> reject(UiMappings.errorText(result.code))
        is RuntimeResult.Unknown -> reject("Runtime 状态未知，已停止登记")
        is RuntimeResult.Success -> reject("Runtime 返回了无法识别的检查结果")
    }
}

internal fun launchInstalledApp(context: Context, packageName: String): Boolean {
    val intent = context.packageManager
        .getLaunchIntentForPackage(packageName)
        ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ?: return false
    return try {
        context.startActivity(intent)
        true
    } catch (_: ActivityNotFoundException) {
        false
    } catch (_: SecurityException) {
        false
    }
}
