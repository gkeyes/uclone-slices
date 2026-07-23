package com.uclone.slots.preview

import com.uclone.slots.preview.model.Destination
import com.uclone.slots.preview.model.InstalledApp
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import com.uclone.slots.preview.runtime.isAck

fun SlotsViewModel.createSlot(packageName: String, name: String, blank: Boolean) =
    launchOperation("创建数据空间", "正在准备 CE + DE 数据") {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || trimmed.length > 48) {
            return@launchOperation reject("名称需要 1–48 个字符")
        }
        coordinator.executeMutationAndLaunch(
            runtime = runtime,
            packageName = packageName,
            requestedSlot = null,
            mutation = { runtime.createSlot(packageName, trimmed, blank) },
            onSnapshot = { snapshot, slotId, mutationResult ->
                confirmAndRefresh(packageName, slotId, snapshot) &&
                    canLaunchAfterSwitch(mutationResult, verificationState, packageName, slotId)
            },
            onLaunch = { launchResult, slotId, mutationResult ->
                record("已创建空间：$trimmed")
                finishConfirmedLaunch(packageName, slotId, mutationResult, launchResult)
            },
            onFailure = {
                revokePackageVerification()
                handleFailure(it, packageName)
            },
        )
    }

fun SlotsViewModel.switchAndOpen(packageName: String, slotId: String) =
    launchOperation("切换数据空间", "暂停 App 并切换 CE + DE") {
        coordinator.executeMutationAndLaunch(
            runtime = runtime,
            packageName = packageName,
            requestedSlot = slotId,
            mutation = { runtime.switchSlot(packageName, slotId) },
            onSnapshot = { snapshot, expectedSlot, mutationResult ->
                confirmAndRefresh(packageName, expectedSlot, snapshot) &&
                    canLaunchAfterSwitch(
                        mutationResult,
                        verificationState,
                        packageName,
                        expectedSlot,
                    )
            },
            onLaunch = { launchResult, expectedSlot, mutationResult ->
                finishConfirmedLaunch(packageName, expectedSlot, mutationResult, launchResult)
            },
            onFailure = {
                revokePackageVerification()
                handleFailure(it, packageName)
            },
        )
    }

fun SlotsViewModel.renameSlot(packageName: String, slotId: String, name: String) =
    launchOperation("重命名空间") {
        when (val result = runtime.rename(packageName, slotId, name.trim())) {
            is RuntimeResult.Success -> if (result.isAck("rename_slot")) {
                refreshPackage(packageName)
            } else handleFailure(RuntimeResult.Unknown("invalid rename result"), packageName)
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.deleteSlot(packageName: String, slotId: String) =
    launchOperation("删除数据空间") {
        when (val result = runtime.delete(packageName, slotId)) {
            is RuntimeResult.Success -> if (result.isAck("delete_slot")) {
                record("已删除非活动空间")
                refreshPackage(packageName)
            } else handleFailure(RuntimeResult.Unknown("invalid delete result"), packageName)
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.reconcile(packageName: String) = launchOperation("重新确认安全状态") {
    when (val result = runtime.reconcile(packageName)) {
        is RuntimeResult.Success -> refreshPackage(packageName)
        else -> handleFailure(result, packageName)
    }
}

fun SlotsViewModel.rescue(packageName: String) = launchOperation("安全退回 Base") {
    when (val result = runtime.rescue(packageName)) {
        is RuntimeResult.Success -> if (result.isAck("rescue_to_base")) {
            apps.forgetManaged(packageName)
            managedApps = managedApps.withoutPackage(packageName)
            revokePackageVerification()
            record("已安全退回 Base")
            navigate(Destination.Apps)
            refreshRuntimeInternal()
        } else handleFailure(RuntimeResult.Unknown("invalid rescue result"), packageName)
        else -> handleFailure(result, packageName)
    }
}

private suspend fun SlotsViewModel.finishConfirmedLaunch(
    packageName: String,
    slotId: String,
    switchResult: RuntimeResult,
    launchResult: RuntimeResult,
) {
    if (!canLaunchAfterSwitch(switchResult, verificationState, packageName, slotId)) {
        revokePackageVerification()
        message = "切换结果或数据视图尚未验证，不会启动目标 App"
        return
    }
    updateOperationPhase("启动已确认的数据空间")
    when (val result = launchResult) {
        is RuntimeResult.Success -> {
            val launched = result.payload as? RuntimePayload.Launch
            if (launched?.packageName == packageName && launched.slotId == slotId) {
                message = launchStatusMessage(launched.launchStatus)
            } else {
                handleFailure(RuntimeResult.Unknown("mismatched launch result"), packageName)
            }
        }
        is RuntimeResult.Unknown -> {
            revokePackageVerification()
            handleFailure(result, packageName)
        }
        is RuntimeResult.Rejected -> {
            revokePackageVerification()
            handleFailure(result, packageName)
        }
    }
}

fun SlotsViewModel.selectRescueTarget(app: InstalledApp) {
    managedApps = managedApps.withRecoveryTarget(app)
    revokePackageVerification()
    destination = Destination.Detail(app.packageName)
}

fun SlotsViewModel.clearMessage() {
    message = null
}

fun SlotsViewModel.dismissEnrollmentReview() {
    enrollmentReview = null
}

internal fun SlotsViewModel.reject(text: String) {
    message = text
}

internal fun SlotsViewModel.record(text: String) {
    taskHistory = (listOf(text) + taskHistory).take(20)
}

internal fun launchStatusMessage(status: String): String? = when (status) {
    "launched" -> null
    "entry_not_found" -> "数据空间已提交，但目标 App 没有可启动入口"
    "gate_blocked" -> "数据空间已提交，但 App 原本不可启动，已保留原状态"
    "failed" -> "数据空间已提交，但系统启动请求失败，请手动打开"
    else -> "数据空间已提交，但 Runtime 返回了未知启动结果"
}
