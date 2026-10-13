package com.uclone.slices.v2.backup

import androidx.annotation.StringRes
import com.uclone.slices.v2.R
import java.util.Locale

/** String resource for a Runtime or archive helper error code shown on the backup pages. */
@StringRes
internal fun backupErrorMessageRes(code: String): Int = when (code) {
    "backup_password_required" -> R.string.backup_password_required
    "backup_auth_failed" -> R.string.backup_auth_failed
    "backup_invalid" -> R.string.backup_invalid
    "backup_incompatible", "identity_mismatch" -> R.string.backup_incompatible
    "insufficient_storage" -> R.string.insufficient_storage
    "io_busy" -> R.string.account_io_busy
    "invalid_request" -> R.string.backup_invalid_input
    "not_found" -> R.string.backup_target_missing
    else -> R.string.backup_restore_operation_failed
}

internal fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = arrayOf("KiB", "MiB", "GiB", "TiB")
    var value = bytes.toDouble()
    var unit = -1
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit += 1
    }
    return String.format(Locale.ROOT, "%.1f %s", value, units[unit])
}

internal fun formatAppVersion(versionName: String, versionCode: Long?): String =
    versionCode?.let { "$versionName ($it)" } ?: versionName
