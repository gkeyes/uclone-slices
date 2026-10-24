package com.uclone.slices.v2.gate

import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
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

    @Test fun aPrepareRequestCarriesOnlyAValidSigner() {
        val ticket = "a".repeat(64)
        val signing = SigningIdentity(SigningKind.Lineage, listOf("ab".repeat(32)))
        val request = RootLaunchClient.prepareRequest(ticket, signing)
        assertEquals("lineage", request.getJSONObject("signing").getString("kind"))
        assertEquals("ab".repeat(32), request.getJSONObject("signing").getJSONArray("sha256").getString(0))
        assertFalse(RootLaunchClient.prepareRequest(ticket, null).has("signing"))
        assertFalse(RootLaunchClient.prepareRequest(ticket, SigningIdentity(SigningKind.Lineage, emptyList())).has("signing"))
    }
}
