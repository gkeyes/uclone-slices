package com.uclone.slices.v2.backup

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.ContextCompat
import com.uclone.slices.v2.runtime.AccountIoScope
import com.uclone.slices.v2.runtime.ArchivedState
import com.uclone.slices.v2.runtime.RestoreBatchResult
import com.uclone.slices.v2.runtime.RestoreMapping
import com.uclone.slices.v2.runtime.SigningIdentity
import java.io.File
import java.util.UUID
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

internal sealed interface BackupRestoreJob {
    val id: String

    data class Backup(
        override val id: String = UUID.randomUUID().toString(),
        val packageName: String,
        val scope: AccountIoScope,
        val destinationUri: String,
        val password: CharArray?,
        val signingKind: com.uclone.slices.v2.runtime.SigningKind,
        val signingSha256: List<String>,
        val appVersion: String,
        val activeAccountId: String?,
        val launchAfterReboot: Boolean,
    ) : BackupRestoreJob

    data class Inspect(
        override val id: String = UUID.randomUUID().toString(),
        val sourceUri: String,
        val password: CharArray?,
    ) : BackupRestoreJob

    data class Restore(
        override val id: String = UUID.randomUUID().toString(),
        val packageName: String,
        val sessionId: String,
        val mappings: List<RestoreMapping>,
        val archivedState: ArchivedState,
        val temporaryEnrollment: Boolean,
        val signingIdentity: SigningIdentity,
    ) : BackupRestoreJob
}

internal enum class BackupJobKind {
    Backup,
    Inspect,
    Restore,
}

internal sealed interface BackupJobState {
    data object Idle : BackupJobState
    data class Running(val kind: BackupJobKind, val completed: Int = 0, val total: Int = 0) :
        BackupJobState
    data class PasswordRequired(val sourceUri: String, val wrongPassword: Boolean) : BackupJobState
    data class Preview(
        val sessionId: String,
        val manifest: ArchiveManifest,
    ) : BackupJobState
    data class BackupComplete(val manifest: ArchiveManifest) : BackupJobState
    data class RestoreComplete(val result: RestoreBatchResult) : BackupJobState
    data class Failure(val code: String) : BackupJobState
}

internal data class RestoreSession(
    val id: String,
    val sourceUri: String,
    val cachedArchive: File,
    val password: CharArray?,
    val manifest: ArchiveManifest,
) {
    fun clear() {
        password?.fill('\u0000')
        cachedArchive.delete()
    }
}

internal object BackupRestoreJobRegistry {
    private val lock = Any()
    private val pending = mutableMapOf<String, BackupRestoreJob>()
    private val sessions = mutableMapOf<String, RestoreSession>()
    private val mutableState = MutableStateFlow<BackupJobState>(BackupJobState.Idle)
    val state: StateFlow<BackupJobState> = mutableState.asStateFlow()

    fun submit(context: Context, job: BackupRestoreJob): Boolean = synchronized(lock) {
        if (mutableState.value is BackupJobState.Running) return@synchronized false
        pending[job.id] = job
        mutableState.value = BackupJobState.Running(job.kind())
        val intent = Intent(context, BackupRestoreService::class.java)
            .putExtra(BackupRestoreService.EXTRA_JOB_ID, job.id)
        runCatching { ContextCompat.startForegroundService(context, intent) }
            .onFailure {
                pending.remove(job.id)?.clearSecrets()
                mutableState.value = BackupJobState.Failure("operation_failed")
            }
            .isSuccess
    }

    fun takeJob(id: String): BackupRestoreJob? = synchronized(lock) { pending.remove(id) }

    fun publish(state: BackupJobState) {
        mutableState.value = state
    }

    fun saveSession(session: RestoreSession) = synchronized(lock) {
        sessions.remove(session.id)?.clear()
        sessions[session.id] = session
    }

    fun takeSession(id: String): RestoreSession? = synchronized(lock) { sessions.remove(id) }

    fun discardSession(id: String) = synchronized(lock) {
        sessions.remove(id)?.clear()
    }

    fun reset() {
        mutableState.value = BackupJobState.Idle
    }

    private fun BackupRestoreJob.kind(): BackupJobKind = when (this) {
        is BackupRestoreJob.Backup -> BackupJobKind.Backup
        is BackupRestoreJob.Inspect -> BackupJobKind.Inspect
        is BackupRestoreJob.Restore -> BackupJobKind.Restore
    }

    private fun BackupRestoreJob.clearSecrets() {
        when (this) {
            is BackupRestoreJob.Backup -> password?.fill('\u0000')
            is BackupRestoreJob.Inspect -> password?.fill('\u0000')
            is BackupRestoreJob.Restore -> Unit
        }
    }
}

internal fun takePersistableReadPermission(context: Context, uri: Uri) {
    runCatching {
        context.contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION,
        )
    }
}

internal fun takePersistableWritePermission(context: Context, uri: Uri) {
    runCatching {
        context.contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
    }
}
