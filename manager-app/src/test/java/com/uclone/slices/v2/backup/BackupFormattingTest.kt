package com.uclone.slices.v2.backup

import com.uclone.slices.v2.R
import kotlin.test.Test
import kotlin.test.assertEquals

class BackupFormattingTest {
    @Test
    fun bytesUseBinaryUnitsWithOneDecimal() {
        assertEquals("0 B", formatBytes(0))
        assertEquals("1023 B", formatBytes(1023))
        assertEquals("1.0 KiB", formatBytes(1024))
        assertEquals("1.5 MiB", formatBytes(1024L * 1024 + 512 * 1024))
        assertEquals("2048.0 TiB", formatBytes(2048L * 1024 * 1024 * 1024 * 1024))
    }

    @Test
    fun appVersionShowsTheCodeWhenKnown() {
        assertEquals("1.2 (34)", formatAppVersion("1.2", 34))
        assertEquals("1.2", formatAppVersion("1.2", null))
    }

    @Test
    fun errorCodesMapToBackupMessages() {
        assertEquals(R.string.backup_incompatible, backupErrorMessageRes("identity_mismatch"))
        assertEquals(R.string.account_io_busy, backupErrorMessageRes("io_busy"))
        assertEquals(
            R.string.backup_restore_operation_failed,
            backupErrorMessageRes("operation_failed"),
        )
    }
}
