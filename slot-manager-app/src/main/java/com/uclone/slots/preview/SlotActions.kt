package com.uclone.slots.preview

import com.uclone.slots.preview.model.Destination
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import com.uclone.slots.preview.runtime.isAck

fun SlotsViewModel.createSlot(packageName: String, name: String, blank: Boolean) =
    launchOperation("创建数据空间", "正在准备 CE + DE 数据") {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || trimmed.length > 48) {
            return@launchOperation reject("名称需要 1–48 个字符")
        }
        when (val result = runtime.createSlot(packageName, trimmed, blank)) {
            is RuntimeResult.Success -> {
                val switched = result.payload as? RuntimePayload.Switch
                if (switched?.packageName != packageName) {
                    return@launchOperation handleFailure(
                        RuntimeResult.Unknown("mismatched package"),
                        packageName,
                    )
                }
                if (confirmAndRefresh(packageName, switched.slotId, launchApp = false)) {
                    record("已创建空间：$trimmed")
                }
            }
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.switchAndOpen(packageName: String, slotId: String) =
    launchOperation("切换数据空间", "暂停 App 并切换 CE + DE") {
        if (selectedStatus?.packageName == packageName && selectedStatus?.activeSlot == slotId) {
            confirmAndRefresh(packageName, slotId, launchApp = true)
            return@launchOperation
        }
        when (val result = runtime.switch(packageName, slotId)) {
            is RuntimeResult.Success -> {
                val switched = result.payload as? RuntimePayload.Switch
                if (switched?.packageName != packageName || switched.slotId != slotId) {
                    return@launchOperation handleFailure(
                        RuntimeResult.Unknown("mismatched switch result"),
                        packageName,
                    )
                }
                confirmAndRefresh(packageName, slotId, launchApp = true)
            }
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.renameSlot(packageName: String, slotId: String, name: String) =
    launchOperation("重命名空间") {
        when (val result = runtime.rename(packageName, slotId, name.trim())) {
            is RuntimeResult.Success -> if (result.isAck("rename_slot")) {
                openDetail(packageName)
            } else handleFailure(RuntimeResult.Unknown("invalid rename result"), packageName)
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.deleteSlot(packageName: String, slotId: String) =
    launchOperation("删除数据空间") {
        when (val result = runtime.delete(packageName, slotId)) {
            is RuntimeResult.Success -> if (result.isAck("delete_slot")) {
                record("已删除非活动空间")
                openDetail(packageName)
            } else handleFailure(RuntimeResult.Unknown("invalid delete result"), packageName)
            else -> handleFailure(result, packageName)
        }
    }

fun SlotsViewModel.reconcile(packageName: String) = launchOperation("重新确认安全状态") {
    when (val result = runtime.reconcile(packageName)) {
        is RuntimeResult.Success -> openDetail(packageName)
        else -> handleFailure(result, packageName)
    }
}

fun SlotsViewModel.rescue(packageName: String) = launchOperation("安全退回 Base") {
    when (val result = runtime.rescue(packageName)) {
        is RuntimeResult.Success -> if (result.isAck("rescue_to_base")) {
            record("已安全退回 Base")
            navigate(Destination.Apps)
            refreshRuntime()
        } else handleFailure(RuntimeResult.Unknown("invalid rescue result"), packageName)
        else -> handleFailure(result, packageName)
    }
}

fun SlotsViewModel.clearMessage() {
    message = null
}

fun SlotsViewModel.dismissEnrollmentReview() {
    enrollmentReview = null
}

fun SlotsViewModel.completePendingLaunch(packageName: String, launched: Boolean) {
    if (pendingLaunchPackage != packageName) return
    pendingLaunchPackage = null
    if (!launched) message = "没有找到可启动入口"
}

fun SlotsViewModel.cancelPendingLaunch(packageName: String) {
    if (pendingLaunchPackage != packageName) return
    pendingLaunchPackage = null
    message = "界面已离开目标 App，已取消自动打开"
}

fun SlotsViewModel.setManagerVisible(visible: Boolean) {
    managerVisible = visible
    if (!visible) pendingLaunchPackage?.let(::cancelPendingLaunch)
}

internal fun SlotsViewModel.reject(text: String) {
    message = text
}

internal fun SlotsViewModel.record(text: String) {
    taskHistory = (listOf(text) + taskHistory).take(20)
}
