package com.uclone.slices.v2.backup

import android.content.Context
import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.runtime.AccountIoScope
import com.uclone.slices.v2.runtime.ArchiveScope
import com.uclone.slices.v2.runtime.ArchivedAccountKind
import com.uclone.slices.v2.runtime.ArchivedState
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.RestoreMapping
import com.uclone.slices.v2.runtime.RestoreTarget
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.RuntimeReply
import com.uclone.slices.v2.runtime.SigningKind
import com.uclone.slices.v2.runtime.SigningIdentity
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal enum class BackupRestorePage {
    Landing,
    BackupSetup,
    RestorePassword,
    RestorePreview,
    Result,
}

internal data class RestoreMappingUi(
    val archiveAccountId: String,
    val archiveKind: ArchiveAccountKind,
    val name: String,
    val target: RestoreTarget,
)

internal data class BackupRestoreUiState(
    val page: BackupRestorePage = BackupRestorePage.Landing,
    val packageName: String? = null,
    val slotId: String? = null,
    val passwordProtected: Boolean = true,
    val password: String = "",
    val passwordConfirmation: String = "",
    val restoreSourceUri: String? = null,
    val manifest: ArchiveManifest? = null,
    val restoreSessionId: String? = null,
    val targetApp: InstalledApp? = null,
    val targetPackage: PackageSnapshot? = null,
    val signatureMatches: Boolean = true,
    val mappings: List<RestoreMappingUi> = emptyList(),
    val destructiveConfirmation: String = "",
    val jobState: BackupJobState = BackupJobState.Idle,
    val errorCode: String? = null,
) {
    val running: Boolean
        get() = jobState is BackupJobState.Running
    val expectedConfirmation: String
        get() = if (signatureMatches) "覆盖" else manifest?.packageName.orEmpty()
}

internal sealed interface BackupUiIntent {
    data object OpenLanding : BackupUiIntent
    data class OpenBackup(val packageName: String, val slotId: String?) : BackupUiIntent
    data class ChooseBackupPackage(val packageName: String) : BackupUiIntent
    data class PasswordProtectionChanged(val enabled: Boolean) : BackupUiIntent
    data class PasswordChanged(val value: String) : BackupUiIntent
    data class PasswordConfirmationChanged(val value: String) : BackupUiIntent
    data class StartBackup(val destination: Uri) : BackupUiIntent
    data class SelectRestore(val source: Uri) : BackupUiIntent
    data object RetryRestorePassword : BackupUiIntent
    data class RestoreMappingChanged(
        val archiveAccountId: String,
        val target: RestoreTarget,
    ) : BackupUiIntent
    data class DestructiveConfirmationChanged(val value: String) : BackupUiIntent
    data object StartRestore : BackupUiIntent
    data object DismissResult : BackupUiIntent
    data object Leave : BackupUiIntent
}

internal class BackupRestoreViewModel(
    private val context: Context,
    private val runtime: RuntimeClient,
    private val installedApps: InstalledAppsSource,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val mutableState = MutableStateFlow(BackupRestoreUiState())
    val state: StateFlow<BackupRestoreUiState> = mutableState.asStateFlow()

    init {
        viewModelScope.launch {
            BackupRestoreJobRegistry.state.collectLatest(::applyJobState)
        }
    }

    fun onIntent(intent: BackupUiIntent) {
        when (intent) {
            BackupUiIntent.OpenLanding -> openLanding()
            is BackupUiIntent.OpenBackup -> openBackup(intent.packageName, intent.slotId)
            is BackupUiIntent.ChooseBackupPackage -> openBackup(intent.packageName, null)
            is BackupUiIntent.PasswordProtectionChanged -> mutableState.update {
                it.copy(
                    passwordProtected = intent.enabled,
                    password = if (intent.enabled) it.password else "",
                    passwordConfirmation = if (intent.enabled) it.passwordConfirmation else "",
                    errorCode = null,
                )
            }
            is BackupUiIntent.PasswordChanged -> mutableState.update {
                it.copy(password = intent.value, errorCode = null)
            }
            is BackupUiIntent.PasswordConfirmationChanged -> mutableState.update {
                it.copy(passwordConfirmation = intent.value, errorCode = null)
            }
            is BackupUiIntent.StartBackup -> startBackup(intent.destination)
            is BackupUiIntent.SelectRestore -> inspectRestore(intent.source, null)
            BackupUiIntent.RetryRestorePassword -> retryRestorePassword()
            is BackupUiIntent.RestoreMappingChanged -> updateMapping(
                intent.archiveAccountId,
                intent.target,
            )
            is BackupUiIntent.DestructiveConfirmationChanged -> mutableState.update {
                it.copy(destructiveConfirmation = intent.value, errorCode = null)
            }
            BackupUiIntent.StartRestore -> startRestore()
            BackupUiIntent.DismissResult -> {
                discardCurrentSession()
                BackupRestoreJobRegistry.reset()
                mutableState.update {
                    BackupRestoreUiState(page = BackupRestorePage.Landing)
                }
            }
            BackupUiIntent.Leave -> leave()
        }
    }

    fun suggestedBackupName(): String {
        val snapshot = state.value
        val stamp = SimpleDateFormat("yyyyMMdd-HHmm", Locale.ROOT).format(Date())
        val suffix = snapshot.slotId ?: "all"
        return "${snapshot.packageName.orEmpty()}-$suffix-$stamp.ucsbackup"
    }

    private fun openLanding() {
        if (state.value.running) return
        discardCurrentSession()
        BackupRestoreJobRegistry.reset()
        mutableState.value = BackupRestoreUiState(page = BackupRestorePage.Landing)
    }

    private fun openBackup(packageName: String, slotId: String?) {
        if (state.value.running) return
        discardCurrentSession()
        BackupRestoreJobRegistry.reset()
        mutableState.value = BackupRestoreUiState(
            page = BackupRestorePage.BackupSetup,
            packageName = packageName,
            slotId = slotId,
        )
    }

    private fun startBackup(destination: Uri) {
        val current = state.value
        if (current.running) return
        val packageName = current.packageName ?: return fail("invalid_request")
        val password = if (current.passwordProtected) {
            if (current.password.isEmpty() || current.password != current.passwordConfirmation) {
                return fail("invalid_request")
            }
            current.password.toCharArray()
        } else {
            null
        }
        takePersistableWritePermission(context, destination)
        viewModelScope.launch(dispatcher) {
            val snapshot = when (val reply = runtime.execute(RuntimeCommand.GetPackage(packageName))) {
                is RuntimeReply.Package -> reply.packageSnapshot
                else -> {
                    password?.fill('\u0000')
                    return@launch fail(runtimeCode(reply))
                }
            }
            val app = installedApps.load().firstOrNull { it.packageName == packageName }
                ?: run {
                    password?.fill('\u0000')
                    return@launch fail("not_found")
                }
            val signing = app.signingIdentity ?: run {
                password?.fill('\u0000')
                return@launch fail("identity_mismatch")
            }
            val slotId = current.slotId
            if (slotId != null && snapshot.slots.none { it.id == slotId }) {
                password?.fill('\u0000')
                return@launch fail("not_found")
            }
            val submitted = BackupRestoreJobRegistry.submit(
                context,
                BackupRestoreJob.Backup(
                    packageName = packageName,
                    scope = slotId?.let { AccountIoScope.Account(it) }
                        ?: AccountIoScope.AllAccounts,
                    destinationUri = destination.toString(),
                    password = password,
                    signingKind = signing.kind,
                    signingSha256 = signing.sha256,
                    appVersion = app.versionName,
                    activeAccountId = slotId ?: snapshot.activeSlot,
                    launchAfterReboot = snapshot.launchAfterReboot,
                ),
            )
            if (!submitted) {
                password?.fill('\u0000')
                fail("io_busy")
            } else {
                mutableState.update {
                    it.copy(
                        password = "",
                        passwordConfirmation = "",
                        jobState = BackupJobState.Running(BackupJobKind.Backup),
                        errorCode = null,
                    )
                }
            }
        }
    }

    private fun inspectRestore(source: Uri, password: CharArray?) {
        if (state.value.running) {
            password?.fill('\u0000')
            return
        }
        takePersistableReadPermission(context, source)
        val submitted = BackupRestoreJobRegistry.submit(
            context,
            BackupRestoreJob.Inspect(
                sourceUri = source.toString(),
                password = password,
            ),
        )
        if (!submitted) {
            password?.fill('\u0000')
            fail("io_busy")
        } else {
            mutableState.update {
                it.copy(
                    restoreSourceUri = source.toString(),
                    password = "",
                    passwordConfirmation = "",
                    jobState = BackupJobState.Running(BackupJobKind.Inspect),
                    errorCode = null,
                )
            }
        }
    }

    private fun retryRestorePassword() {
        val current = state.value
        val source = current.restoreSourceUri ?: return fail("backup_invalid")
        if (current.password.isEmpty()) return fail("backup_password_required")
        inspectRestore(Uri.parse(source), current.password.toCharArray())
    }

    private suspend fun applyJobState(jobState: BackupJobState) {
        when (jobState) {
            is BackupJobState.Preview -> resolvePreview(jobState)
            is BackupJobState.PasswordRequired -> mutableState.update {
                it.copy(
                    page = BackupRestorePage.RestorePassword,
                    restoreSourceUri = jobState.sourceUri,
                    password = "",
                    passwordConfirmation = "",
                    jobState = jobState,
                    errorCode = if (jobState.wrongPassword) "backup_auth_failed" else null,
                )
            }
            is BackupJobState.BackupComplete,
            is BackupJobState.RestoreComplete,
            is BackupJobState.Failure,
            -> mutableState.update {
                it.copy(
                    page = BackupRestorePage.Result,
                    jobState = jobState,
                    errorCode = (jobState as? BackupJobState.Failure)?.code,
                )
            }
            is BackupJobState.Running -> mutableState.update {
                it.copy(jobState = jobState, errorCode = null)
            }
            BackupJobState.Idle -> mutableState.update { it.copy(jobState = jobState) }
        }
    }

    private suspend fun resolvePreview(preview: BackupJobState.Preview) {
        val app = withContext(dispatcher) {
            installedApps.load().firstOrNull { it.packageName == preview.manifest.packageName }
        }
        if (app?.signingIdentity == null) {
            BackupRestoreJobRegistry.discardSession(preview.sessionId)
            mutableState.update {
                it.copy(
                    page = BackupRestorePage.Result,
                    jobState = BackupJobState.Failure("backup_incompatible"),
                    errorCode = "backup_incompatible",
                )
            }
            return
        }
        val packageReply = withContext(dispatcher) {
            runtime.execute(RuntimeCommand.ListPackages)
        }
        val configured = when (packageReply) {
            is RuntimeReply.Packages -> packageReply.packages.firstOrNull {
                it.packageName == preview.manifest.packageName
            }
            else -> {
                BackupRestoreJobRegistry.discardSession(preview.sessionId)
                mutableState.update {
                    it.copy(
                        page = BackupRestorePage.Result,
                        jobState = BackupJobState.Failure(runtimeCode(packageReply)),
                        errorCode = runtimeCode(packageReply),
                    )
                }
                return
            }
        }
        if (configured != null && configured.bindingState != BindingState.Ready) {
            BackupRestoreJobRegistry.discardSession(preview.sessionId)
            mutableState.update {
                it.copy(
                    page = BackupRestorePage.Result,
                    jobState = BackupJobState.Failure("identity_mismatch"),
                    errorCode = "identity_mismatch",
                )
            }
            return
        }
        val usedTargets = mutableSetOf<String>()
        val mappings = preview.manifest.accounts.map { account ->
            val target = when (account.kind) {
                ArchiveAccountKind.Base -> RestoreTarget.Existing("base")
                ArchiveAccountKind.Slot -> configured
                    ?.slots
                    ?.firstOrNull {
                        it.id == account.sourceSlot && it.id != "base" && usedTargets.add(it.id)
                    }
                    ?.let { RestoreTarget.Existing(it.id) }
                    ?: RestoreTarget.New
            }
            RestoreMappingUi(
                archiveAccountId = account.archiveAccountId,
                archiveKind = account.kind,
                name = account.name,
                target = target,
            )
        }
        mutableState.update {
            it.copy(
                page = BackupRestorePage.RestorePreview,
                manifest = preview.manifest,
                restoreSessionId = preview.sessionId,
                targetApp = app,
                targetPackage = configured,
                signatureMatches = archiveSigningMatches(
                    preview.manifest.signingKind,
                    preview.manifest.signingSha256,
                    app.signingIdentity,
                ),
                mappings = mappings,
                destructiveConfirmation = "",
                jobState = preview,
                errorCode = null,
            )
        }
    }

    private fun updateMapping(archiveAccountId: String, target: RestoreTarget) {
        val current = state.value
        val mapping = current.mappings.firstOrNull {
            it.archiveAccountId == archiveAccountId
        } ?: return
        if (mapping.archiveKind == ArchiveAccountKind.Base || target is RestoreTarget.Existing && target.slotId == "base") {
            return
        }
        if (target is RestoreTarget.Existing && current.mappings.any {
                it.archiveAccountId != archiveAccountId &&
                    (it.target as? RestoreTarget.Existing)?.slotId == target.slotId
            }
        ) {
            return fail("invalid_request")
        }
        mutableState.update { state ->
            state.copy(
                mappings = state.mappings.map {
                    if (it.archiveAccountId == archiveAccountId) it.copy(target = target) else it
                },
                errorCode = null,
            )
        }
    }

    private fun startRestore() {
        val current = state.value
        val manifest = current.manifest ?: return fail("backup_invalid")
        val sessionId = current.restoreSessionId ?: return fail("backup_invalid")
        val app = current.targetApp ?: return fail("backup_incompatible")
        val signing = app.signingIdentity ?: return fail("backup_incompatible")
        if (current.destructiveConfirmation != current.expectedConfirmation) {
            return fail("invalid_request")
        }
        val targets = current.mappings.mapNotNull { (it.target as? RestoreTarget.Existing)?.slotId }
        if (targets.distinct().size != targets.size) return fail("invalid_request")
        val mappings = current.mappings.map { mapping ->
            RestoreMapping(
                archiveAccountId = mapping.archiveAccountId,
                archiveKind = if (mapping.archiveKind == ArchiveAccountKind.Base) {
                    ArchivedAccountKind.Base
                } else {
                    ArchivedAccountKind.Slot
                },
                name = mapping.name,
                target = mapping.target,
            )
        }
        val submitted = BackupRestoreJobRegistry.submit(
            context,
            BackupRestoreJob.Restore(
                packageName = manifest.packageName,
                sessionId = sessionId,
                mappings = mappings,
                archivedState = ArchivedState(
                    scope = manifest.scope,
                    activeAccountId = manifest.activeAccountId,
                    launchAfterReboot = manifest.launchAfterReboot,
                ),
                temporaryEnrollment = current.targetPackage == null,
                signingIdentity = signing,
            ),
        )
        if (!submitted) {
            fail("io_busy")
        } else {
            mutableState.update {
                it.copy(
                    jobState = BackupJobState.Running(
                        kind = BackupJobKind.Restore,
                        total = current.mappings.size,
                    ),
                    errorCode = null,
                )
            }
        }
    }

    private fun runtimeCode(reply: RuntimeReply): String = when (reply) {
        is RuntimeReply.Error -> reply.code.wire
        else -> "operation_failed"
    }

    private fun fail(code: String) {
        mutableState.update { it.copy(errorCode = code) }
    }

    private fun discardCurrentSession() {
        state.value.restoreSessionId?.let(BackupRestoreJobRegistry::discardSession)
    }

    private fun leave() {
        if (!state.value.running) {
            discardCurrentSession()
            BackupRestoreJobRegistry.reset()
            mutableState.value = BackupRestoreUiState()
        }
    }

    override fun onCleared() {
        if (!state.value.running) discardCurrentSession()
        super.onCleared()
    }
}

internal fun archiveSigningMatches(
    archiveKind: SigningKind,
    archive: List<String>,
    installed: SigningIdentity?,
): Boolean {
    if (installed == null || archiveKind != installed.kind) return false
    return when (archiveKind) {
        SigningKind.Multiple -> archive == installed.sha256
        SigningKind.Lineage -> archive.any(installed.sha256::contains)
    }
}
