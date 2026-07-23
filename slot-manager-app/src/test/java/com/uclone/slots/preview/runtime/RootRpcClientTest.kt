package com.uclone.slots.preview.runtime

import kotlinx.coroutines.runBlocking
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertIs
import kotlin.test.assertTrue

class RootRpcClientTest {
    @Test
    fun acceptsOnlyTheResponseForTheCurrentRequest() = runBlocking {
        val request = RuntimeRequest(command = "probe", requestId = "apk-current")
        val process = StubProcess(response("apk-current"))

        val result = RootRpcClient { process }.call(request, 1_000)

        assertIs<RuntimeResult.Success>(result)
        Unit
    }

    @Test
    fun staleResponseIdBecomesUnknown() = runBlocking {
        val request = RuntimeRequest(command = "probe", requestId = "apk-current")
        val process = StubProcess(response("apk-stale"))

        val result = RootRpcClient { process }.call(request, 1_000)

        assertIs<RuntimeResult.Unknown>(result)
        Unit
    }

    @Test
    fun duplicateResponseFramesBecomeUnknown() = runBlocking {
        val request = RuntimeRequest(command = "probe", requestId = "apk-current")
        val process = StubProcess(response("apk-current") + response("apk-current"))

        val result = RootRpcClient { process }.call(request, 1_000)

        assertIs<RuntimeResult.Unknown>(result)
        Unit
    }

    @Test
    fun timeoutRemainsUnknownAndTerminatesOnlyTheClientProcess() = runBlocking {
        val request = RuntimeRequest(command = "switch", requestId = "apk-current")
        val process = StubProcess("", completes = false)

        val result = RootRpcClient { process }.call(request, 1)

        assertIs<RuntimeResult.Unknown>(result)
        assertTrue(process.forciblyDestroyed)
    }

    private fun response(requestId: String): String =
        """{"schema_version":2,"request_id":"$requestId","status":"ok","payload":{"kind":"probe_report","data":{"ready":true,"user_unlocked":true,"ce_de_supported":true,"runtime_version":"test","build_id":"test"}}}""" +
            "\n"
}

private class StubProcess(
    stdout: String,
    private val exitCode: Int = 0,
    private val completes: Boolean = true,
) : Process() {
    private val stdin = ByteArrayOutputStream()
    private val stdout = ByteArrayInputStream(stdout.toByteArray())
    private val stderr = ByteArrayInputStream(ByteArray(0))

    var forciblyDestroyed = false
        private set

    override fun getOutputStream(): OutputStream = stdin

    override fun getInputStream(): InputStream = stdout

    override fun getErrorStream(): InputStream = stderr

    override fun waitFor(): Int = exitCode

    override fun waitFor(timeout: Long, unit: TimeUnit): Boolean = completes

    override fun exitValue(): Int {
        if (!completes && !forciblyDestroyed) throw IllegalThreadStateException()
        return exitCode
    }

    override fun destroy() {
        forciblyDestroyed = true
    }

    override fun destroyForcibly(): Process {
        forciblyDestroyed = true
        return this
    }

    override fun isAlive(): Boolean = !completes && !forciblyDestroyed
}
