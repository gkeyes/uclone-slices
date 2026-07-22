package com.uclone.slots.preview

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.uclone.slots.preview.apps.InstalledAppsRepository
import com.uclone.slots.preview.model.*
import com.uclone.slots.preview.runtime.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class SlotsViewModel(application: Application) : AndroidViewModel(application) {
    private val runtime = RuntimeRepository()
    private val apps = InstalledAppsRepository(application)

    var destination by mutableStateOf<Destination>(Destination.Apps)
        private set
    var runtimeHealth by mutableStateOf(RuntimeHealth.Checking)
        private set
    var managedApps by mutableStateOf<List<ManagedApp>>(emptyList())
        private set
    var installedApps by mutableStateOf<List<InstalledApp>>(emptyList())
        private set
    var selectedStatus by mutableStateOf<PackageRuntimeStatus?>(null)
        private set
    var selectedSlots by mutableStateOf<List<SlotSpace>>(emptyList())
        private set
    var operation by mutableStateOf<OperationState?>(null)
        private set
    var message by mutableStateOf<String?>(null)
        private set
    var taskHistory by mutableStateOf<List<String>>(emptyList())
        private set
    var enrollmentReview by mutableStateOf<EnrollmentReview?>(null)
        private set

    init {
        refreshRuntime()
    }

    fun navigate(target: Destination) {
        destination = target
        if (target == Destination.AddApp) loadInstalledApps()
    }

    fun clearMessage() {
        message = null
    }

    fun refreshRuntime() = launchOperation(null) {
        runtimeHealth = RuntimeHealth.Checking
        runtimeHealth = when (val result = runtime.probe()) {
            is RuntimeResult.Success -> UiMappings.probeHealth(result.payload)
            is RuntimeResult.Rejected -> UiMappings.healthForError(result.code)
            is RuntimeResult.Unknown -> RuntimeHealth.DaemonOffline
        }
        if (runtimeHealth == RuntimeHealth.Ready) refreshManagedInternal() else restoreCachedManaged()
    }

    fun openDetail(packageName: String) = launchOperation("读取数据空间") {
        destination = Destination.Detail(packageName)
        loadPackage(packageName)
    }

    fun inspectForEnrollment(app: InstalledApp) = launchOperation("检查 ${app.label}") {
        val inspection = runtime.inspect(app.packageName).payloadAs<RuntimePayload.Inspection>()
            ?: return@launchOperation reject("兼容性检查失败")
        if (inspection.value.blocked) return@launchOperation reject(
            inspection.value.blockerText(),
        )
        enrollmentReview = EnrollmentReview(app, inspection.value)
    }

    fun dismissEnrollmentReview() {
        enrollmentReview = null
    }

    fun confirmEnrollment() {
        val review = enrollmentReview ?: return
        enrollmentReview = null
        launchOperation("登记 ${review.app.label}") {
            when (val result = runtime.enroll(
                review.app.packageName,
                review.inspection.requiresDirectBootConfirmation,
            )) {
                is RuntimeResult.Success -> {
                    record("已登记 ${review.app.label}")
                    refreshManagedInternal()
                    openDetailInternal(review.app.packageName)
                }
                else -> handleFailure(result, review.app.packageName)
            }
        }
    }

    fun createSlot(packageName: String, name: String, blank: Boolean) =
        launchOperation("创建数据空间", "正在准备 CE + DE 数据") {
            val trimmed = name.trim()
            if (trimmed.isEmpty() || trimmed.length > 48) {
                return@launchOperation reject("名称需要 1–48 个字符")
            }
            when (val result = runtime.createSlot(packageName, trimmed, blank)) {
                is RuntimeResult.Success -> {
                    val switched = result.payload as? RuntimePayload.Switch
                    if (switched?.packageName != packageName) {
                        return@launchOperation unknown(packageName)
                    }
                    confirmAndRefresh(packageName, switched.slotId, launchApp = false)
                    record("已创建空间：$trimmed")
                }
                else -> handleFailure(result, packageName)
            }
        }

    fun switchAndOpen(packageName: String, slotId: String) =
        launchOperation("切换数据空间", "暂停 App 并切换 CE + DE") {
            when (val result = runtime.switch(packageName, slotId)) {
                is RuntimeResult.Success -> {
                    val switched = result.payload as? RuntimePayload.Switch
                    if (switched?.packageName != packageName || switched.slotId != slotId) {
                        return@launchOperation unknown(packageName)
                    }
                    confirmAndRefresh(packageName, slotId, launchApp = true)
                }
                else -> handleFailure(result, packageName)
            }
        }

    fun renameSlot(packageName: String, slotId: String, name: String) =
        launchOperation("重命名空间") {
            when (val result = runtime.rename(packageName, slotId, name.trim())) {
                is RuntimeResult.Success -> loadPackage(packageName)
                else -> handleFailure(result, packageName)
            }
        }

    fun deleteSlot(packageName: String, slotId: String) =
        launchOperation("删除数据空间") {
            when (val result = runtime.delete(packageName, slotId)) {
                is RuntimeResult.Success -> {
                    record("已删除非活动空间")
                    loadPackage(packageName)
                }
                else -> handleFailure(result, packageName)
            }
        }

    fun reconcile(packageName: String) = launchOperation("重新确认安全状态") {
        when (val result = runtime.reconcile(packageName)) {
            is RuntimeResult.Success -> loadPackage(packageName)
            else -> handleFailure(result, packageName)
        }
    }

    fun rescue(packageName: String) = launchOperation("安全退回 Base") {
        when (val result = runtime.rescue(packageName)) {
            is RuntimeResult.Success -> {
                record("已安全退回 Base")
                refreshManagedInternal()
                destination = Destination.Apps
            }
            else -> handleFailure(result, packageName)
        }
    }

    private suspend fun confirmAndRefresh(packageName: String, slotId: String, launchApp: Boolean) {
        operation = operation?.copy(phase = "验证 Runtime 提交结果")
        val status = runtime.status(packageName).payloadAs<RuntimePayload.Status>()?.value
        if (status == null || status.activeSlot != slotId || status.requiresRecovery) {
            return unknown(packageName)
        }
        loadPackage(packageName)
        record("已切换到 ${selectedSlots.firstOrNull { it.id == slotId }?.displayName ?: slotId}")
        if (launchApp && status.enabled && !status.suspended && !launchInstalledApp(getApplication(), packageName)) {
            message = "没有找到可启动入口"
        }
        if (launchApp && (!status.enabled || status.suspended)) {
            message = "槽已切换，但 App 原本不可启动，已保留原状态"
        }
    }

    private suspend fun loadPackage(packageName: String, recoverUnknown: Boolean = true) {
        val status = runtime.status(packageName).payloadAs<RuntimePayload.Status>()?.value
        val slots = runtime.listSlots(packageName).payloadAs<RuntimePayload.Slots>()?.rows
        if (status == null || slots == null) {
            if (recoverUnknown) unknown(packageName)
            return
        }
        selectedStatus = status
        selectedSlots = slots
        if (status.requiresRecovery) runtimeHealth = RuntimeHealth.RecoveryRequired
    }

    private suspend fun openDetailInternal(packageName: String) {
        destination = Destination.Detail(packageName)
        loadPackage(packageName)
    }

    private suspend fun refreshManagedInternal() {
        val rows = runtime.listManaged().payloadAs<RuntimePayload.ManagedApps>()?.rows ?: return
        apps.rememberManaged(rows.map { it.packageName })
        managedApps = rows.map {
            ManagedApp(it.packageName, apps.label(it.packageName), it.activeSlot, it.lifecycle)
        }
    }

    private fun restoreCachedManaged() {
        managedApps = apps.cachedManaged().map {
            ManagedApp(it, apps.label(it), "unknown", "recovery_required")
        }
    }

    private fun loadInstalledApps() = viewModelScope.launch(Dispatchers.IO) {
        val rows = apps.launcherApps().filter { app ->
            managedApps.none { it.packageName == app.packageName }
        }
        withContext(Dispatchers.Main) { installedApps = rows }
    }

    private fun launchOperation(
        title: String?,
        phase: String = "正在与 Runtime 通信",
        block: suspend () -> Unit,
    ) = viewModelScope.launch {
        if (title != null) operation = OperationState(title, phase)
        try {
            block()
        } finally {
            operation = null
        }
    }

    private suspend fun handleFailure(result: RuntimeResult, packageName: String) {
        when (result) {
            is RuntimeResult.Rejected -> {
                if (result.code in setOf("recovery_required", "quarantined")) {
                    runtimeHealth = RuntimeHealth.RecoveryRequired
                    message = "状态无法安全证明，App 已保持禁用"
                } else message = UiMappings.errorText(result.code)
            }
            is RuntimeResult.Unknown -> unknown(packageName)
            is RuntimeResult.Success -> Unit
        }
    }

    private suspend fun unknown(packageName: String) {
        operation = operation?.copy(phase = "结果未知，正在重新确认", resultUnknown = true)
        runtime.reconcile(packageName)
        loadPackage(packageName, recoverUnknown = false)
        message = "客户端未取得确定结果，已重新读取 Runtime 状态"
    }

    private fun reject(text: String) {
        message = text
    }

    private fun record(text: String) {
        taskHistory = (listOf(text) + taskHistory).take(20)
    }

}
