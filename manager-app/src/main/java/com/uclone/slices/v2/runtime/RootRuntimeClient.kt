package com.uclone.slices.v2.runtime

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible

class RootRuntimeClient internal constructor(
    private val processStarter: () -> Process = {
        startRootShell(RPC_COMMAND)
    },
) : RuntimeClient {
    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        runInterruptible(Dispatchers.IO) {
            val process = try {
                processStarter()
            } catch (interrupted: InterruptedException) {
                throw interrupted
            } catch (_: Exception) {
                return@runInterruptible RuntimeReply.TransportFailure
            }
            try {
                process.outputStream.use {
                    it.write((RuntimeProtocol.encode(command) + "\n").toByteArray())
                }
                val output = process.inputStream.bufferedReader().use { it.readText() }
                val exitCode = process.waitFor()
                if (exitCode != 0) {
                    RuntimeReply.TransportFailure
                } else {
                    runCatching {
                        RuntimeProtocol.decode(output.trim())
                    }.getOrElse { RuntimeReply.TransportFailure }
                }
            } catch (interrupted: InterruptedException) {
                throw interrupted
            } catch (_: Exception) {
                RuntimeReply.TransportFailure
            } finally {
                if (process.isAlive) process.destroy()
            }
        }

    private companion object {
        const val RPC_COMMAND = "/data/adb/modules/uclone-slices-v2/bin/slotctl rpc"
    }
}
