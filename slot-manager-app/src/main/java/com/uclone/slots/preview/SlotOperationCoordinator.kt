package com.uclone.slots.preview

import com.uclone.slots.preview.model.OperationState
import com.uclone.slots.preview.model.SlotsUiState
import com.uclone.slots.preview.runtime.PackageSnapshot
import com.uclone.slots.preview.runtime.RuntimeGateway
import com.uclone.slots.preview.runtime.RuntimePayload
import com.uclone.slots.preview.runtime.RuntimeResult
import com.uclone.slots.preview.runtime.readPackageSnapshot
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal class SlotOperationCoordinator(
    private val scope: CoroutineScope,
    private val reduce: ((SlotsUiState.() -> SlotsUiState) -> Unit),
    private val onException: (Throwable) -> Unit = {},
) {
    private val operationMutex = Mutex()
    private val queuedOperations = AtomicInteger()

    internal val queueSize: Int
        get() = queuedOperations.get().coerceAtLeast(0)

    internal fun launch(
        title: String?,
        phase: String,
        block: suspend () -> Unit,
    ): Job {
        queuedOperations.incrementAndGet()
        reduce { copy(runtimeBusy = true) }
        val completed = AtomicBoolean(false)
        val job = scope.launch {
            try {
                operationMutex.withLock {
                    if (title != null) {
                        reduce { copy(operation = OperationState(title, phase)) }
                    }
                    try {
                        block()
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (error: Throwable) {
                        onException(error)
                    } finally {
                        reduce { copy(operation = null) }
                    }
                }
            } finally {
                finish(completed)
            }
        }
        job.invokeOnCompletion { finish(completed) }
        return job
    }

    internal fun updatePhase(phase: String) {
        reduce { copy(operation = operation?.copy(phase = phase)) }
    }

    internal fun markResultUnknown(phase: String) {
        reduce {
            copy(operation = operation?.copy(phase = phase, resultUnknown = true))
        }
    }

    internal suspend fun executeMutationAndLaunch(
        runtime: RuntimeGateway,
        packageName: String,
        requestedSlot: String?,
        mutation: suspend () -> RuntimeResult,
        onSnapshot: suspend (PackageSnapshot, String, RuntimeResult) -> Boolean,
        onLaunch: suspend (RuntimeResult, String, RuntimeResult) -> Unit,
        onFailure: suspend (RuntimeResult) -> Unit,
    ) {
        val mutationResult = mutation()
        if (mutationResult !is RuntimeResult.Success) {
            onFailure(mutationResult)
            return
        }
        val switched = mutationResult.payload as? RuntimePayload.Switch
        val slotId = switched?.slotId
        if (switched == null || switched.packageName != packageName ||
            slotId == null || (requestedSlot == null && slotId == "base") ||
            requestedSlot?.let { it != slotId } == true
        ) {
            onFailure(RuntimeResult.Unknown("mismatched mutation result"))
            return
        }
        val snapshot = runtime.readPackageSnapshot(packageName)
        if (snapshot == null || snapshot.status.activeSlot != slotId ||
            snapshot.status.requiresRecovery
        ) {
            onFailure(RuntimeResult.Unknown("invalid post-mutation snapshot"))
            return
        }
        if (!onSnapshot(snapshot, slotId, mutationResult)) return
        when (val launchResult = runtime.launchCurrent(packageName, slotId)) {
            is RuntimeResult.Success -> onLaunch(launchResult, slotId, mutationResult)
            is RuntimeResult.Rejected,
            is RuntimeResult.Unknown,
            -> onFailure(launchResult)
        }
    }

    private fun finish(completed: AtomicBoolean) {
        if (!completed.compareAndSet(false, true)) return
        val remaining = queuedOperations.decrementAndGet().coerceAtLeast(0)
        if (remaining == 0) queuedOperations.set(0)
        reduce { copy(runtimeBusy = remaining > 0) }
    }
}
