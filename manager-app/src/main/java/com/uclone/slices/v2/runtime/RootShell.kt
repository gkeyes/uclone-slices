package com.uclone.slices.v2.runtime

import java.io.File

/**
 * Starts `su -c [command]` with stderr sent to /dev/null, so nothing has to drain it and a chatty
 * command cannot block on a full pipe. This is what `ProcessBuilder.Redirect.DISCARD` does on the
 * JDK; the File form is used because it exists on every API level the Manager supports.
 */
internal fun startRootShell(command: String): Process =
    ProcessBuilder("su", "-c", command)
        .redirectError(File("/dev/null"))
        .start()
