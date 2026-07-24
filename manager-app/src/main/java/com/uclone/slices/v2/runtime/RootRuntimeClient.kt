package com.uclone.slices.v2.runtime

import java.io.OutputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible

class RootRuntimeClient internal constructor(
    private val processStarter: () -> Process = {
        ProcessBuilder("su", "-c", RPC_COMMAND).start()
    },
) : RuntimeClient {
    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        runInterruptible(Dispatchers.IO) {
            val process = try {
                processStarter()
            } catch (_: Exception) {
                return@runInterruptible RuntimeReply.Error(ErrorCode.OperationFailed)
            }
            val stderrReader = Thread {
                process.errorStream.use { it.copyTo(DiscardOutput) }
            }.apply(Thread::start)
            try {
                process.outputStream.use {
                    it.write((RuntimeProtocol.encode(command) + "\n").toByteArray())
                }
                val output = process.inputStream.bufferedReader().use { it.readText() }
                val exitCode = process.waitFor()
                stderrReader.join()
                if (exitCode != 0) {
                    RuntimeReply.Error(ErrorCode.OperationFailed)
                } else {
                    runCatching {
                        RuntimeProtocol.decode(output.trim())
                    }.getOrElse { RuntimeReply.Error(ErrorCode.OperationFailed) }
                }
            } catch (interrupted: InterruptedException) {
                throw interrupted
            } catch (_: Exception) {
                RuntimeReply.Error(ErrorCode.OperationFailed)
            } finally {
                if (process.isAlive) process.destroy()
            }
        }

    private companion object {
        const val RPC_COMMAND = "/data/adb/modules/uclone-slices-v2/bin/slotctl rpc"
    }
}

private object DiscardOutput : OutputStream() {
    override fun write(value: Int) = Unit
}
