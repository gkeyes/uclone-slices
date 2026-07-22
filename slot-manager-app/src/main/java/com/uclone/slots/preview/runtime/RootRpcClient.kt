package com.uclone.slots.preview.runtime

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

interface RpcClient {
    suspend fun call(request: RuntimeRequest, timeoutMs: Long): RuntimeResult
}

class RootRpcClient : RpcClient {
    override suspend fun call(request: RuntimeRequest, timeoutMs: Long): RuntimeResult =
        withContext(Dispatchers.IO) { execute(RuntimeProtocol.encode(request), timeoutMs) }

    private fun execute(frame: String, timeoutMs: Long): RuntimeResult {
        val process = try {
            ProcessBuilder("su", "-c", RPC_COMMAND).start()
        } catch (_: Exception) {
            return RuntimeResult.Unknown("无法启动 Root 客户端")
        }
        val readers = Executors.newFixedThreadPool(2)
        val stdout = readers.submit<ByteArray> { readBounded(process.inputStream) }
        val stderr = readers.submit<ByteArray> { readBounded(process.errorStream) }
        return try {
            process.outputStream.use { it.write(frame.toByteArray(Charsets.UTF_8)) }
            if (!process.waitFor(timeoutMs, TimeUnit.MILLISECONDS)) {
                process.destroyForcibly()
                RuntimeResult.Unknown("客户端等待超时，Runtime 事务结果未知")
            } else {
                val out = stdout.get(STREAM_TIMEOUT_MS, TimeUnit.MILLISECONDS)
                val err = stderr.get(STREAM_TIMEOUT_MS, TimeUnit.MILLISECONDS)
                parseResult(process.exitValue(), out, err)
            }
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            RuntimeResult.Unknown("客户端等待被中断，Runtime 事务结果未知")
        } catch (_: Exception) {
            RuntimeResult.Unknown("无法确认 Runtime 返回结果")
        } finally {
            readers.shutdownNow()
        }
    }

    private fun parseResult(exit: Int, stdout: ByteArray, stderr: ByteArray): RuntimeResult {
        if (stdout.size >= MAX_OUTPUT_BYTES || stderr.size >= MAX_OUTPUT_BYTES) {
            return RuntimeResult.Unknown("Runtime 输出超过安全上限")
        }
        val text = stdout.toString(Charsets.UTF_8)
        if (text.count { it == '\n' } != 1 || !text.endsWith('\n')) {
            return RuntimeResult.Unknown("Runtime 响应格式不完整")
        }
        return try {
            val decoded = RuntimeProtocol.decode(text.trimEnd('\n'))
            if (exit == 0 || decoded is RuntimeResult.Rejected) decoded
            else RuntimeResult.Unknown("Root 客户端异常退出")
        } catch (_: Exception) {
            RuntimeResult.Unknown("Runtime 响应无法解析")
        }
    }

    private fun readBounded(stream: InputStream): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(4096)
        while (true) {
            val count = stream.read(buffer)
            if (count < 0) break
            if (output.size() + count > MAX_OUTPUT_BYTES) return ByteArray(MAX_OUTPUT_BYTES)
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private companion object {
        const val RPC_COMMAND = "/data/adb/modules/uclone-slices-preview/bin/slotctl rpc"
        const val MAX_OUTPUT_BYTES = 64 * 1024
        const val STREAM_TIMEOUT_MS = 2_000L
    }
}
