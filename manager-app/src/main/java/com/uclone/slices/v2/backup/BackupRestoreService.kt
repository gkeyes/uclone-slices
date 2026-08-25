package com.uclone.slices.v2.backup

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.os.StatFs
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.AccountIoKind
import com.uclone.slices.v2.runtime.ArchiveScope
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.RootRuntimeClient
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal class BackupRestoreService : Service() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForegroundNow()
        val jobId = intent?.getStringExtra(EXTRA_JOB_ID)
        val job = jobId?.let(BackupRestoreJobRegistry::takeJob)
        if (job == null) {
            BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
            stopSelf(startId)
            return START_NOT_STICKY
        }
        serviceScope.launch {
            val engine = BackupRestoreEngine(
                applicationContext,
                RootRuntimeClient(),
                RootArchiveHelper(applicationContext),
            )
            try {
                engine.run(job)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Exception) {
                BackupRestoreJobRegistry.publish(
                    BackupJobState.Failure("operation_failed"),
                )
            } finally {
                stopSelf(startId)
            }
        }
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        serviceScope.cancel()
        super.onDestroy()
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
        stopSelf(startId)
    }

    private fun startForegroundNow() {
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_storage)
            .setContentTitle(getString(R.string.backup_restore_notification_title))
            .setContentText(getString(R.string.backup_restore_notification_text))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setProgress(0, 0, true)
            .build()
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            notification,
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
        )
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID,
                    getString(R.string.backup_restore_notification_channel),
                    NotificationManager.IMPORTANCE_LOW,
                ),
            )
        }
    }

    companion object {
        const val EXTRA_JOB_ID = "job_id"
        private const val CHANNEL_ID = "backup_restore"
        private const val NOTIFICATION_ID = 2001
    }
}

internal class BackupRestoreEngine(
    private val context: android.content.Context,
    private val runtime: RuntimeClient,
    private val helper: RootArchiveHelper,
) {
    suspend fun run(job: BackupRestoreJob) {
        when (job) {
            is BackupRestoreJob.Backup -> performBackup(job)
            is BackupRestoreJob.Inspect -> performInspect(job)
            is BackupRestoreJob.Restore -> performRestore(job)
        }
    }

    private suspend fun performBackup(job: BackupRestoreJob.Backup) {
        val destination = Uri.parse(job.destinationUri)
        val lease = when (
            val reply = runtime.execute(RuntimeCommand.BeginBackupIo(job.packageName, job.scope))
        ) {
            is RuntimeReply.AccountIoLeaseReply -> reply.lease
            else -> {
                job.password?.fill('\u0000')
                truncateOrDeleteDocument(destination)
                publishRuntimeFailure(reply)
                return
            }
        }
        if (lease.kind != AccountIoKind.Backup || lease.sources.isEmpty()) {
            job.password?.fill('\u0000')
            withContext(NonCancellable) { abort(lease.ioToken) }
            truncateOrDeleteDocument(destination)
            BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
            return
        }
        var output: File? = null
        var documentComplete = false
        var result: BackupJobState = BackupJobState.Failure("operation_failed")
        try {
            val outputFile = cacheFile("backup-${job.id}.ucsbackup", create = false)
            output = outputFile
            val helperResult = helper.backup(
                BackupArchiveRequest(
                    outputPath = outputFile.absolutePath,
                    password = job.password,
                    packageName = job.packageName,
                    signingKind = job.signingKind,
                    signingSha256 = job.signingSha256,
                    appVersion = job.appVersion,
                    appVersionCode = job.appVersionCode,
                    androidVersion = Build.VERSION.RELEASE,
                    device = "${Build.MANUFACTURER} ${Build.MODEL}",
                    createdAtMillis = System.currentTimeMillis(),
                    scope = if (job.scope is com.uclone.slices.v2.runtime.AccountIoScope.Account) {
                        ArchiveScope.Account
                    } else {
                        ArchiveScope.AllAccounts
                    },
                    activeAccountId = job.activeAccountId,
                    launchAfterReboot = job.launchAfterReboot,
                    sources = lease.sources,
                    profile = job.profile,
                ),
            )
            result = when (helperResult) {
                is ArchiveHelperResult.Success -> {
                    copyToDocument(outputFile, destination)
                    documentComplete = true
                    BackupJobState.BackupComplete(
                        manifest = helperResult.manifest,
                        archiveSize = outputFile.length(),
                    )
                }
                is ArchiveHelperResult.Failure -> BackupJobState.Failure(helperResult.code)
                ArchiveHelperResult.TransportFailure -> BackupJobState.Failure("operation_failed")
            }
        } catch (error: IOException) {
            result = BackupJobState.Failure(ioCode(error))
        } finally {
            output?.delete()
            job.password?.fill('\u0000')
            withContext(NonCancellable) {
                if (!documentComplete) truncateOrDeleteDocument(destination)
                val finalization = runtime.execute(
                    backupFinalizationCommand(lease.ioToken, documentComplete),
                )
                if (finalization != RuntimeReply.Ack) {
                    result = BackupJobState.Failure(runtimeCode(finalization))
                }
            }
        }
        BackupRestoreJobRegistry.publish(result)
    }

    private suspend fun performInspect(job: BackupRestoreJob.Inspect) {
        var input: File? = null
        var retainPassword = false
        try {
            val inputFile = cacheFile("restore-${job.id}.ucsbackup", create = true)
            input = inputFile
            copyFromDocument(Uri.parse(job.sourceUri), inputFile)
            when (val result = helper.inspect(inputFile.absolutePath, job.password)) {
                is ArchiveHelperResult.Success -> {
                    val session = RestoreSession(
                        id = UUID.randomUUID().toString(),
                        sourceUri = job.sourceUri,
                        cachedArchive = inputFile,
                        password = job.password,
                        manifest = result.manifest,
                    )
                    retainPassword = true
                    BackupRestoreJobRegistry.saveSession(session)
                    BackupRestoreJobRegistry.publish(
                        BackupJobState.Preview(session.id, result.manifest),
                    )
                }
                is ArchiveHelperResult.Failure -> {
                    if (
                        result.code == "backup_password_required" ||
                        result.code == "backup_auth_failed"
                    ) {
                        BackupRestoreJobRegistry.publish(
                            BackupJobState.PasswordRequired(
                                sourceUri = job.sourceUri,
                                wrongPassword = result.code == "backup_auth_failed",
                            ),
                        )
                    } else {
                        BackupRestoreJobRegistry.publish(BackupJobState.Failure(result.code))
                    }
                }
                ArchiveHelperResult.TransportFailure -> {
                    BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
                }
            }
        } catch (error: IOException) {
            BackupRestoreJobRegistry.publish(BackupJobState.Failure(ioCode(error)))
        } finally {
            if (!retainPassword) {
                input?.delete()
                job.password?.fill('\u0000')
            }
        }
    }

    private suspend fun performRestore(job: BackupRestoreJob.Restore) {
        val session = BackupRestoreJobRegistry.takeSession(job.sessionId)
        if (session == null) {
            BackupRestoreJobRegistry.publish(BackupJobState.Failure("backup_invalid"))
            return
        }
        var token: String? = null
        var anyRestored = false
        var temporaryEnrollmentCreated = false
        var commitAttempted = false
        var finishAttempted = false
        try {
            if (!hasRestoreSpace(session.manifest.logicalSize)) {
                BackupRestoreJobRegistry.publish(BackupJobState.Failure("insufficient_storage"))
                return
            }
            if (job.temporaryEnrollment) {
                when (val existing = runtime.execute(RuntimeCommand.GetPackage(job.packageName))) {
                    is RuntimeReply.Package -> Unit
                    is RuntimeReply.Error -> {
                        if (existing.code.wire != "not_found") {
                            publishRuntimeFailure(existing)
                            return
                        }
                        val enrolled = runtime.execute(
                            RuntimeCommand.Enroll(job.packageName, job.signingIdentity),
                        )
                        if (enrolled !is RuntimeReply.Package) {
                            publishRuntimeFailure(enrolled)
                            return
                        }
                        temporaryEnrollmentCreated = true
                    }
                    else -> {
                        publishRuntimeFailure(existing)
                        return
                    }
                }
            }
            val transferId = UUID.randomUUID().toString()
            val lease = when (
                val reply = runtime.execute(
                    RuntimeCommand.BeginRestoreIo(
                        packageName = session.manifest.packageName,
                        transferId = transferId,
                        mappings = job.mappings,
                        archivedState = job.archivedState,
                    ),
                )
            ) {
                is RuntimeReply.AccountIoLeaseReply -> reply.lease
                else -> {
                    publishRuntimeFailure(reply)
                    return
                }
            }
            token = lease.ioToken
            if (lease.kind != AccountIoKind.Restore || lease.staging.size != job.mappings.size) {
                BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
                return
            }
            when (
                val extracted = helper.restore(
                    RestoreArchiveRequest(
                        inputPath = session.cachedArchive.absolutePath,
                        password = session.password,
                        staging = lease.staging,
                    ),
                )
            ) {
                is ArchiveHelperResult.Failure -> {
                    BackupRestoreJobRegistry.publish(BackupJobState.Failure(extracted.code))
                    return
                }
                ArchiveHelperResult.TransportFailure -> {
                    BackupRestoreJobRegistry.publish(BackupJobState.Failure("operation_failed"))
                    return
                }
                is ArchiveHelperResult.Success -> {
                    if (extracted.manifest != session.manifest) {
                        BackupRestoreJobRegistry.publish(BackupJobState.Failure("backup_invalid"))
                        return
                    }
                }
            }
            for ((index, mapping) in job.mappings.withIndex()) {
                BackupRestoreJobRegistry.publish(
                    BackupJobState.Running(
                        BackupJobKind.Restore,
                        completed = index,
                        total = job.mappings.size,
                    ),
                )
                commitAttempted = true
                when (
                    val committed = runtime.execute(
                        RuntimeCommand.CommitRestoreAccount(
                            lease.ioToken,
                            mapping.archiveAccountId,
                        ),
                    )
                ) {
                    is RuntimeReply.RestoreItem -> {
                        anyRestored = anyRestored ||
                            committed.result.state ==
                            com.uclone.slices.v2.runtime.RestoreItemState.Restored
                    }
                    else -> {
                        publishRuntimeFailure(committed)
                        return
                    }
                }
            }
            finishAttempted = true
            when (val finished = finishRestoreWithRetry(runtime, lease.ioToken)) {
                is RuntimeReply.RestoreBatch -> {
                    token = null
                    anyRestored = finished.result.items.any {
                        it.state == com.uclone.slices.v2.runtime.RestoreItemState.Restored
                    }
                    BackupRestoreJobRegistry.publish(
                        BackupJobState.RestoreComplete(finished.result),
                    )
                }
                else -> publishRuntimeFailure(finished)
            }
        } catch (error: IOException) {
            BackupRestoreJobRegistry.publish(BackupJobState.Failure(ioCode(error)))
        } finally {
            withContext(NonCancellable) {
                restoreAbortCommand(token, finishAttempted)?.let { runtime.execute(it) }
                cleanupTemporaryEnrollment(
                    job,
                    temporaryEnrollmentCreated,
                    safeToRemove = !commitAttempted || (token == null && !anyRestored),
                )
                session.clear()
            }
        }
    }

    private suspend fun cleanupTemporaryEnrollment(
        job: BackupRestoreJob.Restore,
        temporaryEnrollmentCreated: Boolean,
        safeToRemove: Boolean,
    ) {
        if (temporaryEnrollmentCreated && safeToRemove) {
            runtime.execute(RuntimeCommand.Unenroll(job.packageName))
        }
    }

    private suspend fun abort(token: String) {
        runtime.execute(RuntimeCommand.AbortAccountIo(token))
    }

    private fun publishRuntimeFailure(reply: RuntimeReply) {
        BackupRestoreJobRegistry.publish(BackupJobState.Failure(runtimeCode(reply)))
    }

    private fun runtimeCode(reply: RuntimeReply): String = when (reply) {
        is RuntimeReply.Error -> reply.code.wire
        else -> "operation_failed"
    }

    private fun cacheFile(name: String, create: Boolean): File {
        val directory = File(context.cacheDir, "backup-restore")
        if (!directory.exists() && !directory.mkdir()) throw IOException("cache unavailable")
        directory.setReadable(false, false)
        directory.setWritable(false, false)
        directory.setExecutable(false, false)
        directory.setReadable(true, true)
        directory.setWritable(true, true)
        directory.setExecutable(true, true)
        val file = File(directory, name)
        if (file.exists() && !file.delete()) throw IOException("stale cache unavailable")
        if (create && !file.createNewFile()) throw IOException("cache unavailable")
        file.setReadable(false, false)
        file.setWritable(false, false)
        file.setExecutable(false, false)
        file.setReadable(true, true)
        file.setWritable(true, true)
        return file
    }

    private fun copyFromDocument(uri: Uri, destination: File) {
        context.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(destination, false).use { output -> input.copyTo(output) }
        } ?: throw IOException("document unavailable")
    }

    private fun copyToDocument(source: File, uri: Uri) {
        context.contentResolver.openFileDescriptor(uri, "rwt")?.use { descriptor ->
            FileOutputStream(descriptor.fileDescriptor).use { output ->
                source.inputStream().use { input -> input.copyTo(output) }
                output.fd.sync()
            }
        } ?: throw IOException("document unavailable")
    }

    private fun truncateOrDeleteDocument(uri: Uri) {
        runCatching {
            context.contentResolver.openFileDescriptor(uri, "rwt")?.close()
        }
        runCatching {
            android.provider.DocumentsContract.deleteDocument(context.contentResolver, uri)
        }
    }

    private fun hasRestoreSpace(logicalSize: Long): Boolean {
        val required = logicalSize.coerceAtMost(Long.MAX_VALUE - RESTORE_MARGIN) + RESTORE_MARGIN
        return StatFs(context.cacheDir.absolutePath).availableBytes >= required
    }

    private fun ioCode(error: IOException): String =
        if (error.message?.contains("space", ignoreCase = true) == true) {
            "insufficient_storage"
        } else {
            "operation_failed"
        }

    private companion object {
        const val RESTORE_MARGIN = 64L * 1024 * 1024
    }
}

internal fun backupFinalizationCommand(
    ioToken: String,
    documentComplete: Boolean,
): RuntimeCommand = if (documentComplete) {
    RuntimeCommand.FinishBackupIo(ioToken)
} else {
    RuntimeCommand.AbortAccountIo(ioToken)
}

internal suspend fun finishRestoreWithRetry(
    runtime: RuntimeClient,
    ioToken: String,
): RuntimeReply {
    val command = RuntimeCommand.FinishRestoreIo(ioToken)
    val first = runtime.execute(command)
    val retryable = first is RuntimeReply.Error &&
        first.code in setOf(ErrorCode.StateConflict, ErrorCode.OperationFailed)
    return if (retryable) runtime.execute(command) else first
}

internal fun restoreAbortCommand(
    ioToken: String?,
    finishAttempted: Boolean,
): RuntimeCommand? = if (ioToken != null && !finishAttempted) {
    RuntimeCommand.AbortAccountIo(ioToken)
} else {
    null
}
