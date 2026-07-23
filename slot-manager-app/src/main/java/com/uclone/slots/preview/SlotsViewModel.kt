package com.uclone.slots.preview

import android.app.Application
import androidx.compose.runtime.mutableStateOf
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.uclone.slots.preview.apps.InstalledAppsRepository
import com.uclone.slots.preview.model.*
import com.uclone.slots.preview.runtime.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
class SlotsViewModel(application: Application) : AndroidViewModel(application) {
    internal val runtime: RuntimeGateway = RuntimeRepository()
    internal val apps = InstalledAppsRepository(application)
    private val state = mutableStateOf(SlotsUiState())
    private val stateLock = Any()
    internal val coordinator = SlotOperationCoordinator(viewModelScope, ::updateUiState) { failClosedAfterUnexpectedOperation() }
    val uiState: SlotsUiState get() = state.value
    var destination: Destination
        get() = uiState.destination
        internal set(value) = updateUiState { copy(destination = value) }
    var runtimeHealth: RuntimeHealth
        get() = uiState.runtimeHealth
        private set(value) = updateUiState { copy(runtimeHealth = value) }
    var runtimeMode: RuntimeMode
        get() = uiState.runtimeMode
        private set(value) = updateUiState { copy(runtimeMode = value) }
    var managedApps: List<ManagedApp>
        get() = uiState.managedApps
        internal set(value) = updateUiState { copy(managedApps = value) }
    var installedApps: List<InstalledApp>
        get() = uiState.installedApps
        private set(value) = updateUiState { copy(installedApps = value) }
    var selectedStatus: PackageRuntimeStatus?
        get() = uiState.selectedStatus
        internal set(value) = updateUiState { copy(selectedStatus = value) }
    var selectedSlots: List<SlotSpace>
        get() = uiState.selectedSlots
        internal set(value) = updateUiState { copy(selectedSlots = value) }
    var verificationState: VerificationState
        get() = uiState.verificationState
        internal set(value) = updateUiState { copy(verificationState = value) }
    val operation: OperationState? get() = uiState.operation
    val runtimeBusy: Boolean get() = uiState.runtimeBusy
    var message: String?
        get() = uiState.message
        internal set(value) = updateUiState { copy(message = value) }
    var taskHistory: List<String>
        get() = uiState.taskHistory
        internal set(value) = updateUiState { copy(taskHistory = value) }
    var enrollmentReview: EnrollmentReview?
        get() = uiState.enrollmentReview
        internal set(value) = updateUiState { copy(enrollmentReview = value) }
    init { refreshRuntime() }
    internal fun updateUiState(reducer: SlotsUiState.() -> SlotsUiState) {
        synchronized(stateLock) { state.value = reducer(state.value) }
    }
    fun navigate(target: Destination) {
        destination = target
        if (target == Destination.AddApp || target == Destination.RescueApp) {
            loadInstalledApps(includeManaged = target == Destination.RescueApp)
        }
    }
    fun refreshRuntime() = launchOperation(null) { refreshRuntimeInternal() }
    internal suspend fun refreshRuntimeInternal() {
        runtimeHealth = RuntimeHealth.Checking
        val probeState = UiMappings.probeState(runtime.probe())
        runtimeHealth = probeState.health
        runtimeMode = probeState.mode
        when {
            runtimeMode == RuntimeMode.RecoveryOnly -> {
                val targets = runtime.listRecoveryTargets()
                    .payloadAs<RuntimePayload.RecoveryTargets>()?.packageNames
                if (targets == null) restoreCachedManaged() else {
                    apps.rememberManaged(targets)
                    managedApps = recoveryManagedApps(targets, apps::label)
                }
            }
            runtimeHealth == RuntimeHealth.Ready -> if (!refreshManagedInternal()) {
                restoreCachedManaged()
                message = "Runtime 可连接，但无法刷新完整列表；仍可逐个读取已知应用"
            }
            else -> restoreCachedManaged()
        }
    }
    fun openDetail(packageName: String) {
        destination = Destination.Detail(packageName)
        if (
            runtimeHealth == RuntimeHealth.DaemonOffline ||
            runtimeHealth == RuntimeHealth.PairMismatch
        ) {
            revokePackageVerification()
            return
        }
        launchOperation("读取数据空间") { loadPackage(packageName) }
    }
    fun inspectForEnrollment(app: InstalledApp) = launchOperation("检查 ${app.label}") {
        val result = runtime.inspect(app.packageName)
        if (result !is RuntimeResult.Success) {
            return@launchOperation handleInspectionFailure(result)
        }
        val inspection = (result.payload as? RuntimePayload.Inspection)?.value
            ?: return@launchOperation reject("Runtime 返回了无法识别的检查结果")
        if (inspection.packageName != app.packageName) {
            return@launchOperation reject("Runtime 返回了其他 App 的检查结果，已停止登记")
        }
        if (inspection.blocked) return@launchOperation reject(
            inspection.blockerText(),
        )
        enrollmentReview = EnrollmentReview(app, inspection)
    }
    fun confirmEnrollment() {
        val review = enrollmentReview ?: return
        enrollmentReview = null
        launchOperation("登记 ${review.app.label}") {
            when (val result = runtime.enroll(
                review.app.packageName,
                review.inspection.requiresDirectBootConfirmation,
            )) {
                is RuntimeResult.Success -> if (result.isAck("enroll_package")) {
                    record("已登记 ${review.app.label}")
                    refreshManagedInternal()
                    openDetailInternal(review.app.packageName)
                } else {
                    handleFailure(
                        RuntimeResult.Unknown("invalid enrollment result"),
                        review.app.packageName,
                    )
                }
                else -> handleFailure(result, review.app.packageName)
            }
        }
    }
    internal suspend fun confirmAndRefresh(
        packageName: String,
        slotId: String,
    ): Boolean {
        updateOperationPhase("验证 Runtime 提交结果")
        val snapshot = runtime.readPackageSnapshot(packageName)
        if (snapshot == null) {
            revokePackageVerification()
            unknown(packageName)
            return false
        }
        return confirmAndRefresh(packageName, slotId, snapshot)
    }
    internal suspend fun confirmAndRefresh(
        packageName: String,
        slotId: String,
        snapshot: PackageSnapshot,
    ): Boolean {
        updateOperationPhase("验证 Runtime 提交结果")
        if (snapshot.status.packageName != packageName || snapshot.status.activeSlot != slotId ||
            snapshot.status.requiresRecovery
        ) {
            revokePackageVerification()
            return false
        }
        applyVerifiedSnapshot(snapshot, VerificationSource.PostMutation)
        record("已切换到 ${selectedSlots.firstOrNull { it.id == slotId }?.displayName ?: slotId}")
        return true
    }
    internal fun updateOperationPhase(phase: String) {
        coordinator.updatePhase(phase)
    }
    private suspend fun loadPackage(packageName: String, recoverUnknown: Boolean = true): Boolean {
        if (runtimeMode == RuntimeMode.RecoveryOnly) {
            revokePackageVerification()
            return true
        }
        if (selectedStatus?.packageName != packageName) {
            revokePackageVerification()
        }
        val snapshot = queryPackageForMode(runtimeMode, packageName, runtime::readPackageSnapshot)
        if (snapshot == null) {
            if (recoverUnknown) unknown(packageName)
            return false
        }
        applyVerifiedSnapshot(snapshot, VerificationSource.Snapshot)
        return true
    }
    internal suspend fun openDetailInternal(packageName: String) {
        destination = Destination.Detail(packageName)
        loadPackage(packageName)
    }
    internal suspend fun refreshPackage(packageName: String) = loadPackage(packageName)
    private suspend fun refreshManagedInternal(): Boolean {
        val rows = runtime.listManaged().payloadAs<RuntimePayload.ManagedApps>()?.rows
            ?: return false
        apps.rememberManaged(rows.map { it.packageName })
        managedApps = rows.map {
            ManagedApp(it.packageName, apps.label(it.packageName), it.activeSlot, it.lifecycle)
        }
        return true
    }
    private fun restoreCachedManaged() {
        managedApps = apps.cachedManaged().map {
            ManagedApp(it, apps.label(it), "unknown", PackageLifecycle.RecoveryRequired)
        }
    }
    private fun loadInstalledApps(includeManaged: Boolean) = viewModelScope.launch(Dispatchers.IO) {
        val rows = apps.launcherApps(includeDisabled = includeManaged).filter { app -> includeManaged ||
            managedApps.none { it.packageName == app.packageName } }
        withContext(Dispatchers.Main) { installedApps = rows }
    }
    internal fun launchOperation(
        title: String?,
        phase: String = "正在与 Runtime 通信",
        block: suspend () -> Unit,
    ): Job = coordinator.launch(title, phase, block)
    internal suspend fun handleFailure(result: RuntimeResult, packageName: String) {
        when (result) {
            is RuntimeResult.Rejected -> {
                if (result.code in setOf("recovery_required", "quarantined")) {
                    revokePackageVerification()
                    markPackageLifecycle(
                        packageName,
                        if (result.code == "quarantined") PackageLifecycle.Quarantined
                        else PackageLifecycle.RecoveryRequired,
                    )
                    message = "Runtime 已要求隔离该 App；请重新读取状态确认门禁"
                } else {
                    if (result.code == "runtime_pair_mismatch") {
                        runtimeHealth = RuntimeHealth.PairMismatch
                    }
                    message = UiMappings.errorText(result.code)
                }
            }
            is RuntimeResult.Unknown -> unknown(packageName)
            is RuntimeResult.Success -> Unit
        }
    }
    private suspend fun unknown(packageName: String) {
        revokePackageVerification()
        coordinator.markResultUnknown("结果未知，正在重新确认")
        val reconciled = runtime.reconcile(packageName)
        val loaded = reconciled is RuntimeResult.Success &&
            loadPackage(packageName, recoverUnknown = false)
        verificationState = verificationAfterUnknown(reconciled, verificationState)
        message = if (loaded && selectedStatus?.requiresRecovery == false) {
            "客户端未取得原操作结果，已重新读取安全状态；不会自动启动 App"
        } else {
            revokePackageVerification()
            markPackageLifecycle(packageName, PackageLifecycle.RecoveryRequired)
            "无法证明最终数据视图，也无法确认 App 当前是否已禁用；不会自动启动"
        }
    }
}
