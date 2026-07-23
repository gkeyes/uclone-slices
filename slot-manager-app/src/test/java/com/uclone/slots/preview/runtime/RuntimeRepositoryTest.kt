package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.BuildConfig
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RuntimeRepositoryTest {
    @Test
    fun directBootConsentIsForwardedOnlyWhenConfirmed() = runBlocking {
        val client = RecordingClient()
        val repository = RuntimeRepository(client)

        repository.enroll("com.example.normal", false)
        assertFalse(client.requests.last().acceptDirectBootConditional ?: true)

        client.requests.clear()
        repository.enroll("com.example.directboot", true)
        assertTrue(client.requests.last().acceptDirectBootConditional == true)
    }

    @Test
    fun repositoryNeverTurnsConsentIntoAPathOrShellValue() = runBlocking {
        val client = RecordingClient()
        val repository = RuntimeRepository(client)

        repository.enroll("com.example.app", true)

        val request = client.requests.last()
        assertEquals("enroll_package", request.command)
        assertEquals("com.example.app", request.packageName)
        assertEquals(null, request.slotId)
        assertEquals(null, request.displayName)
        assertEquals(null, request.seedMode)
    }

    @Test
    fun daemonPairMismatchIsPreservedWithoutAnExtraProbe() = runBlocking {
        val client = RecordingClient(mutationError = "runtime_pair_mismatch")
        val repository = RuntimeRepository(client)

        val result = repository.switchSlot("com.example.app", "preview")

        assertEquals(RuntimeResult.Rejected("runtime_pair_mismatch"), result)
        assertEquals(listOf("switch"), client.requests.map { it.command })
    }

    @Test
    fun confirmedSwitchThenLaunchFlowUsesThreeTypedCalls() = runBlocking {
        val client = RecordingClient()
        val repository = RuntimeRepository(client)

        repository.switchSlot("com.example.app", "work")
        repository.packageSnapshot("com.example.app")
        repository.launchCurrent("com.example.app", "work")

        assertEquals(
            listOf("switch", "package_snapshot", "launch_current"),
            client.requests.map { it.command },
        )
        assertEquals("com.example.app", client.requests.last().packageName)
        assertEquals("work", client.requests.last().slotId)
    }

    @Test
    fun invalidProbePayloadIsRejectedByExplicitProbe() = runBlocking {
        val client = RecordingClient(probePayload = RuntimePayload.Ack("probe"))
        val repository = RuntimeRepository(client)

        val result = repository.probe()

        assertEquals(RuntimeResult.Rejected("invalid_probe"), result)
        assertEquals(listOf("probe"), client.requests.map { it.command })
    }

    @Test
    fun explicitProbeReportsRuntimeReadinessWithoutPrefacingMutation() = runBlocking {
        val client = RecordingClient(ready = false)
        val repository = RuntimeRepository(client)

        val result = repository.probe()

        assertTrue(result is RuntimeResult.Success)
        assertEquals(listOf("probe"), client.requests.map { it.command })
    }

    @Test
    fun pairedRecoveryOnlyRuntimeCanListTargetsWithoutBeingReady() = runBlocking {
        val client = RecordingClient(ready = false)
        val repository = RuntimeRepository(client)

        repository.listRecoveryTargets()

        assertEquals(listOf("list_recovery_targets"), client.requests.map { it.command })
    }

    @Test
    fun pairingMismatchFromDaemonBlocksRecoveryTargetDiscovery() = runBlocking {
        val client = RecordingClient(mutationError = "runtime_pair_mismatch", ready = false)
        val repository = RuntimeRepository(client)

        val result = repository.listRecoveryTargets()

        assertEquals(RuntimeResult.Rejected("runtime_pair_mismatch"), result)
        assertEquals(listOf("list_recovery_targets"), client.requests.map { it.command })
    }

    @Test
    fun offlineRescueDoesNotDependOnProbeOrPairing() = runBlocking {
        val client = RecordingClient(buildId = "mismatched-runtime")
        val repository = RuntimeRepository(client)

        val result = repository.rescue("com.example.app")

        assertTrue(result is RuntimeResult.Success)
        assertEquals(listOf("rescue_to_base"), client.requests.map { it.command })
    }
}

private class RecordingClient(
    private val buildId: String = BuildConfig.PREVIEW_BUILD_ID,
    private val ready: Boolean = true,
    private val probePayload: RuntimePayload? = null,
    private val mutationError: String? = null,
) : RpcClient {
    val requests = mutableListOf<RuntimeRequest>()

    override suspend fun call(request: RuntimeRequest, timeoutMs: Long): RuntimeResult {
        requests += request
        if (request.command == "probe") {
            return RuntimeResult.Success(
                probePayload ?: RuntimePayload.Probe(ready, true, true, "test", buildId),
            )
        }
        mutationError?.let { return RuntimeResult.Rejected(it) }
        return RuntimeResult.Success(RuntimePayload.Ack("recorded"))
    }
}
