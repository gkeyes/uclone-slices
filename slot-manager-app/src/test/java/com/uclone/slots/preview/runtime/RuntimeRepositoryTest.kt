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
    fun pairMismatchBlocksMutationBeforeItReachesRuntime() = runBlocking {
        val client = RecordingClient(buildId = "other-build")
        val repository = RuntimeRepository(client)

        val result = repository.switch("com.example.app", "preview")

        assertEquals(RuntimeResult.Rejected("runtime_pair_mismatch"), result)
        assertEquals(listOf("probe"), client.requests.map { it.command })
    }

    @Test
    fun invalidProbePayloadBlocksMutation() = runBlocking {
        val client = RecordingClient(probePayload = RuntimePayload.Ack("probe"))
        val repository = RuntimeRepository(client)

        val result = repository.switch("com.example.app", "preview")

        assertEquals(RuntimeResult.Rejected("invalid_probe"), result)
        assertEquals(listOf("probe"), client.requests.map { it.command })
    }

    @Test
    fun runtimeThatIsNotReadyBlocksMutation() = runBlocking {
        val client = RecordingClient(ready = false)
        val repository = RuntimeRepository(client)

        val result = repository.switch("com.example.app", "preview")

        assertEquals(RuntimeResult.Rejected("unsupported_device"), result)
        assertEquals(listOf("probe"), client.requests.map { it.command })
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
) : RpcClient {
    val requests = mutableListOf<RuntimeRequest>()

    override suspend fun call(request: RuntimeRequest, timeoutMs: Long): RuntimeResult {
        requests += request
        if (request.command == "probe") {
            return RuntimeResult.Success(
                probePayload ?: RuntimePayload.Probe(ready, true, true, "test", buildId),
            )
        }
        return RuntimeResult.Success(RuntimePayload.Ack("recorded"))
    }
}
