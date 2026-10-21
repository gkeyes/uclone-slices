package com.uclone.slices.v2.gate

import com.uclone.slices.v2.runtime.startRootShell
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import org.json.JSONObject

internal data class PreparedLaunch(val packageName: String, val permit: String, val permitExtra: String)

/** Ticket/permit values travel only in private pipes and memory, never shell arguments. */
internal class RootLaunchClient(
    private val processStarter: () -> Process = {
        startRootShell("/data/adb/modules/uclone-slices-v2/bin/slotctl gate")
    },
) {
    suspend fun prepare(ticket: String): PreparedLaunch? = runInterruptible(Dispatchers.IO) {
        if (!validTicket(ticket)) return@runInterruptible null
        val request = JSONObject().put("op", "prepare").put("ticket", ticket)
        val response = exchange(request)?.optJSONObject("ok") ?: return@runInterruptible null
        decodePrepared(response)
    }

    fun cancelAsync(ticket: String) {
        if (!validTicket(ticket)) return
        Thread({ exchange(JSONObject().put("op", "cancel").put("ticket", ticket)) }, "Slice cancel launch")
            .apply { isDaemon = true; start() }
    }

    private fun exchange(request: JSONObject): JSONObject? {
        val process = runCatching { processStarter() }.getOrNull() ?: return null
        return try {
            process.outputStream.use { it.write((request.toString() + "\n").toByteArray()) }
            val text = process.inputStream.bufferedReader().use { reader ->
                val buffer = StringBuilder()
                while (buffer.length <= MAX_FRAME) {
                    val value = reader.read()
                    if (value < 0 || value == '\n'.code) break
                    buffer.append(value.toChar())
                }
                buffer.toString()
            }
            if (text.length > MAX_FRAME) return null
            if (process.waitFor() != 0) null else JSONObject(text)
        } catch (_: Exception) {
            null
        } finally {
            if (process.isAlive) process.destroy()
        }
    }

    internal companion object {
        private const val MAX_FRAME = 64 * 1024
        fun validTicket(ticket: String): Boolean = ticket.matches(Regex("[0-9a-f]{64}"))

        fun decodePrepared(value: JSONObject): PreparedLaunch? {
            val packageName = value.optString("package")
            val permit = value.optString("permit")
            val extra = value.optString("permit_extra")
            if (!packageName.matches(Regex("[A-Za-z0-9_]+(?:\\.[A-Za-z0-9_]+)+")) ||
                !validTicket(permit) || extra != LaunchPolicyCache.PERMIT_EXTRA
            ) return null
            return PreparedLaunch(packageName, permit, extra)
        }
    }
}
