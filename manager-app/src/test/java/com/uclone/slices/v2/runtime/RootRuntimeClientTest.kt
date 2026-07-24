package com.uclone.slices.v2.runtime

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals

class RootRuntimeClientTest {
    @Test
    fun oneProcessCarriesOneProtocolFrame() = runBlocking {
        val process = CompletedProcess(
            """{"ok":{"build_id":"test"}}""" + "\n",
        )
        val client = RootRuntimeClient { process }

        val reply = client.execute(RuntimeCommand.Probe)

        assertEquals(RuntimeReply.Capabilities("test"), reply)
        assertEquals(
            """{"op":"probe"}""" + "\n",
            process.writtenText(),
        )
    }

    @Test
    fun processStartFailureIsTransportFailure() = runBlocking {
        val client = RootRuntimeClient { error("su unavailable") }

        assertEquals(
            RuntimeReply.TransportFailure,
            client.execute(RuntimeCommand.Probe),
        )
    }

    @Test
    fun nonZeroExitIsTransportFailure() = runBlocking {
        val client = RootRuntimeClient {
            CompletedProcess(response = "", exitCode = 1)
        }

        assertEquals(
            RuntimeReply.TransportFailure,
            client.execute(RuntimeCommand.Probe),
        )
    }

    @Test
    fun malformedFrameIsTransportFailure() = runBlocking {
        val client = RootRuntimeClient {
            CompletedProcess(response = "not-json")
        }

        assertEquals(
            RuntimeReply.TransportFailure,
            client.execute(RuntimeCommand.Probe),
        )
    }

    @Test
    fun validWireOperationFailureRemainsAConnectedRuntimeReply() = runBlocking {
        val client = RootRuntimeClient {
            CompletedProcess("""{"error":{"code":"operation_failed"}}""")
        }

        assertEquals(
            RuntimeReply.Error(ErrorCode.OperationFailed),
            client.execute(RuntimeCommand.Probe),
        )
    }
}

private class CompletedProcess(
    response: String,
    private val exitCode: Int = 0,
) : Process() {
    private val input = ByteArrayInputStream(response.toByteArray())
    private val output = ByteArrayOutputStream()

    override fun getOutputStream(): OutputStream = output

    override fun getInputStream(): InputStream = input

    override fun getErrorStream(): InputStream = ByteArrayInputStream(ByteArray(0))

    override fun waitFor(): Int = exitCode

    override fun exitValue(): Int = exitCode

    override fun destroy() = Unit

    fun writtenText(): String = output.toByteArray().toString(Charsets.UTF_8)
}
