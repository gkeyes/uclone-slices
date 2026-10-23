package com.uclone.slices.v2.gate

import kotlin.test.Test
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking

class RootLaunchClientTest {
    @Test fun aSilentRootHelperIsEndedAtTheDeadline() {
        val client = RootLaunchClient(
            processStarter = { ProcessBuilder("sleep", "30").start() },
            deadlineMillis = 200,
        )
        val started = System.nanoTime()
        assertNull(runBlocking { client.prepare("a".repeat(64)) })
        assertTrue((System.nanoTime() - started) / 1_000_000 < 10_000)
    }
}
