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
}

private class CompletedProcess(response: String) : Process() {
    private val input = ByteArrayInputStream(response.toByteArray())
    private val output = ByteArrayOutputStream()

    override fun getOutputStream(): OutputStream = output

    override fun getInputStream(): InputStream = input

    override fun getErrorStream(): InputStream = ByteArrayInputStream(ByteArray(0))

    override fun waitFor(): Int = 0

    override fun exitValue(): Int = 0

    override fun destroy() = Unit

    fun writtenText(): String = output.toByteArray().toString(Charsets.UTF_8)
}
