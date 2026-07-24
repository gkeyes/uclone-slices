package com.uclone.slices.v2.ui

import androidx.lifecycle.ViewModel
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SeedMode
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SlotsUiState(
    val runtimeReady: Boolean = false,
    val buildId: String = "",
    val installedApps: List<InstalledApp> = emptyList(),
    val packages: List<PackageSnapshot> = emptyList(),
    val selected: PackageSnapshot? = null,
    val slotName: String = "",
    val seed: SeedMode = SeedMode.Blank,
    val busy: Boolean = false,
    val message: String? = null,
)

sealed interface UiIntent {
    data object Refresh : UiIntent
    data class OpenPackage(val packageName: String) : UiIntent
    data object BackToPackages : UiIntent
    data class SlotNameChanged(val value: String) : UiIntent
    data class SeedChanged(val value: SeedMode) : UiIntent
    data object CreateSlot : UiIntent
    data class ActivateSlot(val slotId: String) : UiIntent
}

class SlotsViewModel(
    private val client: RuntimeClient,
    private val installedApps: InstalledAppsSource,
    dispatcher: CoroutineDispatcher,
) : ViewModel() {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val mutableState = MutableStateFlow(SlotsUiState())
    private val operationActive = AtomicBoolean(false)
    val state: StateFlow<SlotsUiState> = mutableState.asStateFlow()

    init {
        runOperation { refresh() }
    }

    fun onIntent(intent: UiIntent) {
        when (intent) {
            UiIntent.Refresh -> runOperation { refresh() }
            is UiIntent.OpenPackage -> {
                val packageName = intent.packageName
                val enrolled = state.value.packages.any { it.packageName == packageName }
                val command = if (enrolled) {
                    RuntimeCommand.GetPackage(packageName)
                } else {
                    RuntimeCommand.Enroll(packageName)
                }
                runOperation { applyPackageReply(client.execute(command)) }
            }
            UiIntent.BackToPackages -> mutableState.update {
                it.copy(selected = null, slotName = "", seed = SeedMode.Blank)
            }
            is UiIntent.SlotNameChanged ->
                mutableState.update { it.copy(slotName = intent.value) }
            is UiIntent.SeedChanged -> mutableState.update { it.copy(seed = intent.value) }
            UiIntent.CreateSlot -> {
                val snapshot = state.value
                val packageName = snapshot.selected?.packageName ?: return
                val name = snapshot.slotName.trim()
                if (name.isEmpty()) return
                val seed = snapshot.seed
                runOperation {
                    applyPackageReply(
                        client.execute(RuntimeCommand.CreateSlot(packageName, name, seed)),
                    )
                }
            }
            is UiIntent.ActivateSlot -> {
                val packageName = state.value.selected?.packageName ?: return
                val slotId = intent.slotId
                runOperation {
                    applyPackageReply(
                        client.execute(RuntimeCommand.ActivateSlot(packageName, slotId)),
                    )
                }
            }
        }
    }

    override fun onCleared() {
        scope.cancel()
    }

    private fun runOperation(operation: suspend () -> Unit) {
        if (!operationActive.compareAndSet(false, true)) return
        mutableState.update { it.copy(busy = true, message = null) }
        scope.launch {
            try {
                operation()
            } finally {
                operationActive.set(false)
                mutableState.update { it.copy(busy = false) }
            }
        }
    }

    private suspend fun refresh() {
        val localApps = runCatching(installedApps::load).getOrElse {
            mutableState.update { state ->
                state.copy(installedApps = emptyList(), message = "读取本地应用失败")
            }
            emptyList()
        }
        mutableState.update { it.copy(installedApps = localApps) }
        when (val probe = client.execute(RuntimeCommand.Probe)) {
            is RuntimeReply.Capabilities -> {
                mutableState.update {
                    it.copy(runtimeReady = true, buildId = probe.buildId)
                }
                refreshPackages()
            }
            is RuntimeReply.Error -> showError(probe.code)
            is RuntimeReply.Package,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private suspend fun refreshPackages() {
        when (val reply = client.execute(RuntimeCommand.ListPackages)) {
            is RuntimeReply.Packages -> mutableState.update { current ->
                val selectedPackage = current.selected?.packageName
                current.copy(
                    packages = reply.packages.sortedBy(PackageSnapshot::packageName),
                    selected = reply.packages.firstOrNull {
                        it.packageName == selectedPackage
                    },
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            is RuntimeReply.Capabilities,
            is RuntimeReply.Package,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun applyPackageReply(reply: RuntimeReply) {
        when (reply) {
            is RuntimeReply.Package -> mutableState.update { current ->
                val packages = current.packages
                    .filterNot { it.packageName == reply.packageSnapshot.packageName } +
                    reply.packageSnapshot
                current.copy(
                    packages = packages.sortedBy(PackageSnapshot::packageName),
                    selected = reply.packageSnapshot,
                    slotName = "",
                    seed = if (reply.packageSnapshot.activeSlot == "base") {
                        current.seed
                    } else {
                        SeedMode.Blank
                    },
                    message = null,
                )
            }
            is RuntimeReply.Error -> showError(reply.code)
            is RuntimeReply.Capabilities,
            is RuntimeReply.Packages,
            -> showError(ErrorCode.OperationFailed)
        }
    }

    private fun showError(code: ErrorCode) {
        val message = when (code) {
            ErrorCode.InvalidRequest -> "输入无效"
            ErrorCode.NotFound -> "应用或数据槽不存在"
            ErrorCode.StateConflict -> "应用记录与当前数据状态不一致，请刷新后重试"
            ErrorCode.OperationFailed -> "Runtime 操作失败"
        }
        mutableState.update {
            it.copy(
                runtimeReady = if (code == ErrorCode.OperationFailed) false else it.runtimeReady,
                message = message,
            )
        }
    }
}
