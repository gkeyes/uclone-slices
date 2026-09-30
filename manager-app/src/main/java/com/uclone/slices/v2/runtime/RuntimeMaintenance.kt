package com.uclone.slices.v2.runtime

import java.io.OutputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible

/**
 * User-triggered recovery for the Runtime process itself. Restarting reuses the module's own
 * `service.sh`, which leaves a healthy Runtime alone and otherwise replaces a dead or stale one.
 */
interface RuntimeMaintenance {
    suspend fun restartRuntime(): Boolean

    suspend fun readLogTail(): String?
}

internal class NoRuntimeMaintenance : RuntimeMaintenance {
    override suspend fun restartRuntime(): Boolean = false

    override suspend fun readLogTail(): String? = null
}

class RootRuntimeMaintenance internal constructor(
    private val processStarter: (String) -> Process = { command ->
        ProcessBuilder("su", "-c", command).start()
    },
) : RuntimeMaintenance {
    override suspend fun restartRuntime(): Boolean =
        run(RESTART_COMMAND)?.let { it.exitCode == 0 } ?: false

    override suspend fun readLogTail(): String? =
        run(LOG_COMMAND)?.takeIf { it.exitCode == 0 }?.stdout

    private suspend fun run(command: String): CommandResult? = runInterruptible(Dispatchers.IO) {
        val process = try {
            processStarter(command)
        } catch (interrupted: InterruptedException) {
            throw interrupted
        } catch (_: Exception) {
            return@runInterruptible null
        }
        try {
            process.outputStream.close()
            val stderrReader = Thread {
                process.errorStream.use { it.copyTo(MaintenanceDiscardOutput) }
            }.apply(Thread::start)
            val stdout = process.inputStream.bufferedReader().use { it.readText() }
            val exitCode = process.waitFor()
            stderrReader.join()
            CommandResult(exitCode, stdout)
        } catch (interrupted: InterruptedException) {
            throw interrupted
        } catch (_: Exception) {
            null
        } finally {
            if (process.isAlive) process.destroy()
        }
    }

    private data class CommandResult(val exitCode: Int, val stdout: String)

    private companion object {
        const val MODULE = "/data/adb/modules/uclone-slices-v2"
        const val RUNTIME_ROOT = "/data/adb/uclone-slices-v2"

        // service.sh returns right after starting ucloned in the background; wait until its
        // socket appears or that process is gone, whichever happens first.
        const val RESTART_COMMAND =
            "[ -f $MODULE/service.sh ] && [ ! -f $MODULE/disable ] && " +
                "/system/bin/sh $MODULE/service.sh && " +
                "PID=\$(cat $RUNTIME_ROOT/ucloned.pid 2>/dev/null); " +
                "while [ ! -S $RUNTIME_ROOT/runtime.sock ] && [ -n \"\$PID\" ] && " +
                "kill -0 \"\$PID\" 2>/dev/null; do sleep 0.2; done; " +
                "[ -S $RUNTIME_ROOT/runtime.sock ]"

        const val LOG_COMMAND =
            "for f in $RUNTIME_ROOT/ucloned.log.1 $RUNTIME_ROOT/ucloned.log; do " +
                "[ -f \"\$f\" ] && tail -n 40 \"\$f\"; done; true"
    }
}

private object MaintenanceDiscardOutput : OutputStream() {
    override fun write(value: Int) = Unit
}
