package com.uclone.slices.v2.backup

import android.content.Context
import com.uclone.slices.v2.BuildConfig
import java.io.DataOutputStream
import java.io.File
import java.io.OutputStream
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible

internal class RootArchiveHelper(
    private val context: Context,
    private val processStarter: (String) -> Process = { command ->
        ProcessBuilder("su", "-c", command).start()
    },
) {
    suspend fun backup(request: BackupArchiveRequest): ArchiveHelperResult =
        execute(ArchiveHelperProtocol.encodeBackup(request))

    suspend fun inspect(inputPath: String, password: CharArray?): ArchiveHelperResult =
        execute(ArchiveHelperProtocol.encodeInspect(inputPath, password))

    suspend fun restore(request: RestoreArchiveRequest): ArchiveHelperResult =
        execute(ArchiveHelperProtocol.encodeRestore(request))

    private suspend fun execute(control: ByteArray): ArchiveHelperResult =
        runInterruptible(Dispatchers.IO) {
            try {
                if (!installVerifiedHelper()) return@runInterruptible ArchiveHelperResult.TransportFailure
                val process = processStarter(HELPER_COMMAND)
                val stderrReader = Thread {
                    process.errorStream.use { it.copyTo(DiscardOutput) }
                }.apply(Thread::start)
                try {
                    DataOutputStream(process.outputStream).use { output ->
                        output.writeInt(control.size)
                        output.write(control)
                    }
                    control.fill(0)
                    val response = process.inputStream.bufferedReader().use { reader ->
                        reader.readLine()?.take(MAX_RESPONSE_CHARS + 1)
                    }
                    val exitCode = process.waitFor()
                    stderrReader.join()
                    if (response == null || response.length > MAX_RESPONSE_CHARS) {
                        ArchiveHelperResult.TransportFailure
                    } else {
                        val decoded = ArchiveHelperProtocol.decode(response)
                        if (exitCode == 0 || decoded is ArchiveHelperResult.Failure) {
                            decoded
                        } else {
                            ArchiveHelperResult.TransportFailure
                        }
                    }
                } finally {
                    control.fill(0)
                    if (process.isAlive) process.destroy()
                }
            } catch (interrupted: InterruptedException) {
                control.fill(0)
                throw interrupted
            } catch (_: Exception) {
                control.fill(0)
                ArchiveHelperResult.TransportFailure
            }
        }

    private fun installVerifiedHelper(): Boolean {
        val expected = BuildConfig.ARCHIVE_HELPER_SHA256
        if (!expected.matches(Regex("[0-9a-f]{64}"))) return false
        val bundled = File(context.applicationInfo.nativeLibraryDir, BUNDLED_NAME)
        if (!bundled.isFile || sha256(bundled) != expected) return false
        val source = shellQuote(bundled.absolutePath)
        val destinationDirectory = shellQuote(HELPER_DIRECTORY)
        val destination = shellQuote(HELPER_PATH)
        val command = buildString {
            append("umask 077 && mkdir -p ")
            append(destinationDirectory)
            append(" && chown 0:0 ")
            append(destinationDirectory)
            append(" && chmod 0700 ")
            append(destinationDirectory)
            append(" && cp ")
            append(source)
            append(' ')
            append(destination)
            append(" && chown 0:0 ")
            append(destination)
            append(" && chmod 0700 ")
            append(destination)
            append(" && sha256sum ")
            append(destination)
        }
        val process = processStarter(command)
        process.outputStream.close()
        val stderrReader = Thread {
            process.errorStream.use { it.copyTo(DiscardOutput) }
        }.apply(Thread::start)
        val output = process.inputStream.bufferedReader().use { it.readLine().orEmpty() }
        val exitCode = process.waitFor()
        stderrReader.join()
        return exitCode == 0 && output.substringBefore(' ') == expected
    }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { byte ->
            "%02x".format(byte.toInt() and 0xff)
        }
    }

    private fun shellQuote(value: String): String = "'${value.replace("'", "'\\''")}'"

    private companion object {
        const val MAX_RESPONSE_CHARS = 4 * 1024 * 1024
        const val BUNDLED_NAME = "libuclone_archive.so"
        const val HELPER_DIRECTORY =
            "/data/adb/uclone-slices-v2/manager-helper/${BuildConfig.VERSION_NAME}"
        const val HELPER_PATH = "$HELPER_DIRECTORY/uclone_archive"
        const val HELPER_COMMAND = "exec /system/bin/nsenter -t 1 -m -- $HELPER_PATH"
    }
}

private object DiscardOutput : OutputStream() {
    override fun write(value: Int) = Unit
}
