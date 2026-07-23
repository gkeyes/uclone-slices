package com.uclone.slots.preview

import com.uclone.slots.preview.model.SlotsUiState
import java.util.Collections
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking

class SlotOperationCoordinatorTest {
    @Test
    fun queuedOperationsAreSerializedAndBusyUntilTheQueueDrains() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val stateLock = Any()
        var state = SlotsUiState()
        val coordinator = SlotOperationCoordinator(
            scope = scope,
            reduce = { reducer ->
                synchronized(stateLock) { state = reducer(state) }
            },
        )
        val firstStarted = CompletableDeferred<Unit>()
        val releaseFirst = CompletableDeferred<Unit>()
        val active = AtomicInteger()
        val peak = AtomicInteger()
        val order = Collections.synchronizedList(mutableListOf<String>())
        val first = coordinator.launch("first", "starting") {
            order += "first-start"
            peak.updateAndGet { maxOf(it, active.incrementAndGet()) }
            firstStarted.complete(Unit)
            releaseFirst.await()
            active.decrementAndGet()
            order += "first-end"
        }
        firstStarted.await()
        val second = coordinator.launch("second", "starting") {
            order += "second-start"
            peak.updateAndGet { maxOf(it, active.incrementAndGet()) }
            active.decrementAndGet()
            order += "second-end"
        }
        assertTrue(state.runtimeBusy)
        assertEquals(2, coordinator.queueSize)
        releaseFirst.complete(Unit)
        first.join()
        second.join()
        scope.cancel()
        assertEquals(1, peak.get())
        assertEquals(listOf("first-start", "first-end", "second-start", "second-end"), order)
        assertFalse(state.runtimeBusy)
        assertEquals(0, coordinator.queueSize)
        assertNull(state.operation)
    }

    @Test
    fun exceptionStillClearsOperationBusyAndQueue() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        var state = SlotsUiState()
        val failure = AtomicReference<Throwable?>()
        val coordinator = SlotOperationCoordinator(
            scope = scope,
            reduce = { reducer -> state = reducer(state) },
            onException = { failure.set(it) },
        )
        val job = coordinator.launch("fails", "starting") {
            error("test failure")
        }
        job.join()
        scope.cancel()
        assertEquals("test failure", failure.get()?.message)
        assertFalse(state.runtimeBusy)
        assertEquals(0, coordinator.queueSize)
        assertNull(state.operation)
    }

    @Test
    fun unknownPhaseKeepsTheResultUnknownMarker() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        var state = SlotsUiState()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val coordinator = SlotOperationCoordinator(
            scope = scope,
            reduce = { reducer -> state = reducer(state) },
        )
        val job = coordinator.launch("switch", "starting") {
            coordinator.markResultUnknown("结果未知")
            entered.complete(Unit)
            release.await()
        }
        entered.await()
        assertTrue(state.operation?.resultUnknown == true)
        assertEquals("结果未知", state.operation?.phase)
        release.complete(Unit)
        job.join()
        scope.cancel()
    }
}
