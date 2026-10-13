package com.uclone.slices.v2.runtime

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield

class RuntimeRepositoryTest {
    @Test
    fun commandsFromSeveralCallersNeverOverlap() = runBlocking {
        val transport = CountingTransport()
        val repository = RuntimeRepository(transport)

        val replies = List(8) { index ->
            async(Dispatchers.Default) {
                repository.execute(
                    if (index % 2 == 0) RuntimeCommand.Probe else RuntimeCommand.ListPackages,
                )
            }
        }.awaitAll()

        assertEquals(8, transport.calls)
        assertEquals(1, transport.maxInFlight)
        assertEquals(8, replies.size)
    }

    @Test
    fun aSecondCallerWaitsForTheCommandInFlight() = runBlocking {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val order = mutableListOf<String>()
        val repository = RuntimeRepository(
            object : RuntimeClient {
                override suspend fun execute(command: RuntimeCommand): RuntimeReply {
                    order += "start:$command"
                    if (command == RuntimeCommand.Probe) {
                        started.complete(Unit)
                        release.await()
                    }
                    order += "end:$command"
                    return RuntimeReply.Ack
                }
            },
        )

        val first = async { repository.execute(RuntimeCommand.Probe) }
        started.await()
        val second = async { repository.execute(RuntimeCommand.ListPackages) }
        yield()
        release.complete(Unit)
        first.await()
        second.await()

        assertEquals(
            listOf(
                "start:${RuntimeCommand.Probe}",
                "end:${RuntimeCommand.Probe}",
                "start:${RuntimeCommand.ListPackages}",
                "end:${RuntimeCommand.ListPackages}",
            ),
            order,
        )
    }

    private class CountingTransport : RuntimeClient {
        private val lock = Any()
        private var inFlight = 0
        var maxInFlight = 0
            private set
        var calls = 0
            private set

        override suspend fun execute(command: RuntimeCommand): RuntimeReply {
            synchronized(lock) {
                inFlight += 1
                calls += 1
                maxInFlight = maxOf(maxInFlight, inFlight)
            }
            Thread.sleep(2)
            yield()
            synchronized(lock) { inFlight -= 1 }
            return RuntimeReply.Ack
        }
    }
}
