package com.uclone.slices.v2.runtime

import com.uclone.slices.v2.BuildConfig
import org.json.JSONArray
import org.json.JSONObject

enum class SeedMode(val wire: String) {
    Blank("blank"),
    CloneBase("clone_base"),
}

enum class SigningKind(val wire: String) {
    Lineage("lineage"),
    Multiple("multiple"),
}

data class SigningIdentity(
    val kind: SigningKind,
    val sha256: List<String>,
) {
    fun isValid(): Boolean =
        sha256.isNotEmpty() &&
            sha256.size <= 16 &&
            sha256.distinct().size == sha256.size &&
            sha256.all { digest ->
                digest.length == 64 && digest.all { it in '0'..'9' || it in 'a'..'f' }
            } &&
            (kind != SigningKind.Multiple || (
                sha256.size >= 2 && sha256 == sha256.sorted()
            ))
}

enum class BindingState(val wire: String) {
    Ready("ready"),
    LegacyUnbound("legacy_unbound"),
    RebindRequired("rebind_required"),
    LegacyConfirmationRequired("legacy_confirmation_required");

    companion object {
        fun fromWire(value: String): BindingState = entries.first { it.wire == value }
    }
}

data class SlotSnapshot(
    val id: String,
    val name: String,
)

data class PackageSnapshot(
    val packageName: String,
    val activeSlot: String,
    val slots: List<SlotSnapshot>,
    val launchAfterReboot: Boolean = false,
    val bindingState: BindingState = BindingState.LegacyUnbound,
)

sealed interface AccountIoScope {
    data class Account(val slotId: String) : AccountIoScope
    data object AllAccounts : AccountIoScope
}

enum class AccountIoKind(val wire: String) {
    Backup("backup"),
    Restore("restore");

    companion object {
        fun fromWire(value: String): AccountIoKind = entries.first { it.wire == value }
    }
}

enum class ArchiveScope(val wire: String) {
    Account("account"),
    AllAccounts("all_accounts"),
}

enum class ArchivedAccountKind(val wire: String) {
    Base("base"),
    Slot("slot"),
}

sealed interface RestoreTarget {
    data class Existing(val slotId: String) : RestoreTarget
    data object New : RestoreTarget
}

data class RestoreMapping(
    val archiveAccountId: String,
    val archiveKind: ArchivedAccountKind,
    val name: String,
    val target: RestoreTarget,
)

data class ArchivedState(
    val scope: ArchiveScope,
    val activeAccountId: String?,
    val launchAfterReboot: Boolean,
)

data class AccountIoSource(
    val archiveAccountId: String,
    val slotId: String,
    val name: String,
    val cePath: String,
    val dePath: String,
)

data class RestoreStaging(
    val archiveAccountId: String,
    val cePath: String,
    val dePath: String,
)

data class AccountIoLease(
    val ioToken: String,
    val packageName: String,
    val kind: AccountIoKind,
    val sources: List<AccountIoSource>,
    val staging: List<RestoreStaging>,
)

data class AccountIoStatus(
    val ioToken: String,
    val packageName: String,
    val kind: AccountIoKind,
    val phase: String,
    val completed: List<RestoreItemResult>,
)

enum class RestoreItemState(val wire: String) {
    Restored("restored"),
    Failed("failed"),
    Skipped("skipped");

    companion object {
        fun fromWire(value: String): RestoreItemState = entries.first { it.wire == value }
    }
}

data class RestoreItemResult(
    val archiveAccountId: String,
    val targetSlot: String?,
    val state: RestoreItemState,
)

data class RestoreBatchResult(
    val packageName: String,
    val items: List<RestoreItemResult>,
    val activeSlot: String,
    val launchAfterReboot: Boolean,
    val appStarted: Boolean,
)

sealed interface RuntimeCommand {
    data object Probe : RuntimeCommand
    data object ListPackages : RuntimeCommand
    data class GetPackage(val packageName: String) : RuntimeCommand
    data class Enroll(
        val packageName: String,
        val signing: SigningIdentity,
    ) : RuntimeCommand
    data class RebindPackage(
        val packageName: String,
        val signing: SigningIdentity,
        val trustLegacy: Boolean,
    ) : RuntimeCommand
    data class Unenroll(val packageName: String) : RuntimeCommand
    data class SetLaunchAfterReboot(
        val packageName: String,
        val enabled: Boolean,
    ) : RuntimeCommand
    data class CreateSlot(
        val packageName: String,
        val name: String,
        val seed: SeedMode,
    ) : RuntimeCommand
    data class ActivateSlot(val packageName: String, val slotId: String) : RuntimeCommand
    data class RenameSlot(
        val packageName: String,
        val slotId: String,
        val name: String,
    ) : RuntimeCommand
    data class DeleteSlot(val packageName: String, val slotId: String) : RuntimeCommand
    data class BeginBackupIo(
        val packageName: String,
        val scope: AccountIoScope,
    ) : RuntimeCommand
    data class FinishBackupIo(val ioToken: String) : RuntimeCommand
    data class BeginRestoreIo(
        val packageName: String,
        val transferId: String,
        val mappings: List<RestoreMapping>,
        val archivedState: ArchivedState,
    ) : RuntimeCommand
    data class CommitRestoreAccount(
        val ioToken: String,
        val archiveAccountId: String,
    ) : RuntimeCommand
    data class FinishRestoreIo(val ioToken: String) : RuntimeCommand
    data class AbortAccountIo(val ioToken: String) : RuntimeCommand
    data object ListAccountIoStatus : RuntimeCommand
}

enum class ErrorCode(val wire: String) {
    InvalidRequest("invalid_request"),
    NotFound("not_found"),
    StateConflict("state_conflict"),
    IdentityMismatch("identity_mismatch"),
    OperationFailed("operation_failed"),
    IoBusy("io_busy"),
    BackupInvalid("backup_invalid"),
    BackupPasswordRequired("backup_password_required"),
    BackupAuthFailed("backup_auth_failed"),
    InsufficientStorage("insufficient_storage"),
    BackupIncompatible("backup_incompatible");

    companion object {
        fun fromWire(value: String): ErrorCode =
            entries.first { it.wire == value }
    }
}

sealed interface RuntimeReply {
    data class Capabilities(val buildId: String) : RuntimeReply
    data class Packages(val packages: List<PackageSnapshot>) : RuntimeReply
    data class Package(val packageSnapshot: PackageSnapshot) : RuntimeReply
    data class AccountIoLeaseReply(val lease: AccountIoLease) : RuntimeReply
    data class AccountIoStatuses(val statuses: List<AccountIoStatus>) : RuntimeReply
    data class RestoreItem(val result: RestoreItemResult) : RuntimeReply
    data class RestoreBatch(val result: RestoreBatchResult) : RuntimeReply
    data object Ack : RuntimeReply
    data class Error(val code: ErrorCode) : RuntimeReply
    data object TransportFailure : RuntimeReply
}

object RuntimeProtocol {
    fun encode(command: RuntimeCommand): String {
        val json = when (command) {
            RuntimeCommand.Probe -> JSONObject().put("op", "probe")
            RuntimeCommand.ListPackages -> JSONObject().put("op", "list_packages")
            is RuntimeCommand.GetPackage -> JSONObject()
                .put("op", "get_package")
                .put("package", command.packageName)
            is RuntimeCommand.Enroll -> JSONObject()
                .put("op", "enroll")
                .put("package", command.packageName)
                .put("signing", encodeSigning(command.signing))
            is RuntimeCommand.RebindPackage -> JSONObject()
                .put("op", "rebind_package")
                .put("package", command.packageName)
                .put("signing", encodeSigning(command.signing))
                .put("trust_legacy", command.trustLegacy)
            is RuntimeCommand.Unenroll -> JSONObject()
                .put("op", "unenroll")
                .put("package", command.packageName)
            is RuntimeCommand.SetLaunchAfterReboot -> JSONObject()
                .put("op", "set_launch_after_reboot")
                .put("package", command.packageName)
                .put("enabled", command.enabled)
            is RuntimeCommand.CreateSlot -> JSONObject()
                .put("op", "create_slot")
                .put("package", command.packageName)
                .put("name", command.name)
                .put("seed", command.seed.wire)
            is RuntimeCommand.ActivateSlot -> JSONObject()
                .put("op", "activate_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
            is RuntimeCommand.RenameSlot -> JSONObject()
                .put("op", "rename_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
                .put("name", command.name)
            is RuntimeCommand.DeleteSlot -> JSONObject()
                .put("op", "delete_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
            is RuntimeCommand.BeginBackupIo -> JSONObject()
                .put("op", "begin_backup_io")
                .put("package", command.packageName)
                .put("scope", encodeAccountIoScope(command.scope))
            is RuntimeCommand.FinishBackupIo -> JSONObject()
                .put("op", "finish_backup_io")
                .put("io_token", command.ioToken)
            is RuntimeCommand.BeginRestoreIo -> JSONObject()
                .put("op", "begin_restore_io")
                .put("package", command.packageName)
                .put("transfer_id", command.transferId)
                .put("mappings", JSONArray(command.mappings.map(::encodeRestoreMapping)))
                .put("archived_state", encodeArchivedState(command.archivedState))
            is RuntimeCommand.CommitRestoreAccount -> JSONObject()
                .put("op", "commit_restore_account")
                .put("io_token", command.ioToken)
                .put("archive_account_id", command.archiveAccountId)
            is RuntimeCommand.FinishRestoreIo -> JSONObject()
                .put("op", "finish_restore_io")
                .put("io_token", command.ioToken)
            is RuntimeCommand.AbortAccountIo -> JSONObject()
                .put("op", "abort_account_io")
                .put("io_token", command.ioToken)
            RuntimeCommand.ListAccountIoStatus -> JSONObject()
                .put("op", "list_account_io_status")
        }
        if (command != RuntimeCommand.Probe) {
            json.put("client_build_id", BuildConfig.VERSION_NAME)
        }
        return json.toString()
    }

    private fun encodeSigning(signing: SigningIdentity): JSONObject = JSONObject()
        .put("kind", signing.kind.wire)
        .put("sha256", JSONArray(signing.sha256))

    private fun encodeAccountIoScope(scope: AccountIoScope): JSONObject = when (scope) {
        is AccountIoScope.Account -> JSONObject()
            .put("kind", "account")
            .put("slot", scope.slotId)
        AccountIoScope.AllAccounts -> JSONObject().put("kind", "all_accounts")
    }

    private fun encodeRestoreMapping(mapping: RestoreMapping): JSONObject = JSONObject()
        .put("archive_account_id", mapping.archiveAccountId)
        .put("archive_kind", mapping.archiveKind.wire)
        .put("name", mapping.name)
        .put(
            "target",
            when (val target = mapping.target) {
                is RestoreTarget.Existing -> JSONObject()
                    .put("kind", "existing")
                    .put("slot", target.slotId)
                RestoreTarget.New -> JSONObject().put("kind", "new")
            },
        )

    private fun encodeArchivedState(state: ArchivedState): JSONObject = JSONObject()
        .put("scope", state.scope.wire)
        .put("active_account_id", state.activeAccountId ?: JSONObject.NULL)
        .put("launch_after_reboot", state.launchAfterReboot)

    fun decode(frame: String): RuntimeReply {
        val root = JSONObject(frame)
        root.optJSONObject("error")?.let { error ->
            return RuntimeReply.Error(ErrorCode.fromWire(error.getString("code")))
        }
        val ok = root.getJSONObject("ok")
        if (ok.has("build_id")) {
            return RuntimeReply.Capabilities(
                buildId = ok.getString("build_id"),
            )
        }
        if (ok.has("packages")) {
            return RuntimeReply.Packages(parsePackages(ok.getJSONArray("packages")))
        }
        if (ok.has("io_token")) {
            return RuntimeReply.AccountIoLeaseReply(parseAccountIoLease(ok))
        }
        if (ok.has("account_io")) {
            return RuntimeReply.AccountIoStatuses(
                parseAccountIoStatuses(ok.getJSONArray("account_io")),
            )
        }
        if (ok.has("archive_account_id") && ok.has("state")) {
            return RuntimeReply.RestoreItem(parseRestoreItem(ok))
        }
        if (ok.has("items") && ok.has("active_slot")) {
            return RuntimeReply.RestoreBatch(parseRestoreBatch(ok))
        }
        if (ok.length() == 0) {
            return RuntimeReply.Ack
        }
        return RuntimeReply.Package(parsePackage(ok.getJSONObject("package")))
    }

    private fun parsePackages(array: JSONArray): List<PackageSnapshot> =
        List(array.length()) { index -> parsePackage(array.getJSONObject(index)) }

    private fun parsePackage(json: JSONObject): PackageSnapshot =
        PackageSnapshot(
            packageName = json.getString("package"),
            activeSlot = json.getString("active_slot"),
            slots = parseSlots(json.getJSONArray("slots")),
            launchAfterReboot = json.optBoolean("launch_after_reboot", false),
            bindingState = json.optString("binding_state")
                .takeIf { it.isNotEmpty() }
                ?.let(BindingState::fromWire)
                ?: BindingState.LegacyUnbound,
        )

    private fun parseSlots(array: JSONArray): List<SlotSnapshot> =
        List(array.length()) { index ->
            val slot = array.getJSONObject(index)
            SlotSnapshot(
                id = slot.getString("id"),
                name = slot.getString("name"),
            )
        }

    private fun parseAccountIoLease(json: JSONObject): AccountIoLease = AccountIoLease(
        ioToken = json.getString("io_token"),
        packageName = json.getString("package"),
        kind = AccountIoKind.fromWire(json.getString("kind")),
        sources = json.optJSONArray("sources")?.let { array ->
            List(array.length()) { index ->
                val source = array.getJSONObject(index)
                AccountIoSource(
                    archiveAccountId = source.getString("archive_account_id"),
                    slotId = source.getString("slot"),
                    name = source.getString("name"),
                    cePath = source.getString("ce_path"),
                    dePath = source.getString("de_path"),
                )
            }
        } ?: emptyList(),
        staging = json.optJSONArray("staging")?.let { array ->
            List(array.length()) { index ->
                val item = array.getJSONObject(index)
                RestoreStaging(
                    archiveAccountId = item.getString("archive_account_id"),
                    cePath = item.getString("ce_path"),
                    dePath = item.getString("de_path"),
                )
            }
        } ?: emptyList(),
    )

    private fun parseAccountIoStatuses(array: JSONArray): List<AccountIoStatus> =
        List(array.length()) { index ->
            val status = array.getJSONObject(index)
            AccountIoStatus(
                ioToken = status.getString("io_token"),
                packageName = status.getString("package"),
                kind = AccountIoKind.fromWire(status.getString("kind")),
                phase = status.get("phase").toString(),
                completed = status.optJSONArray("completed")?.let { completed ->
                    List(completed.length()) { item ->
                        parseRestoreItem(completed.getJSONObject(item))
                    }
                } ?: emptyList(),
            )
        }

    private fun parseRestoreItem(json: JSONObject): RestoreItemResult = RestoreItemResult(
        archiveAccountId = json.getString("archive_account_id"),
        targetSlot = if (json.isNull("target_slot")) null else json.getString("target_slot"),
        state = RestoreItemState.fromWire(json.getString("state")),
    )

    private fun parseRestoreBatch(json: JSONObject): RestoreBatchResult = RestoreBatchResult(
        packageName = json.getString("package"),
        items = json.getJSONArray("items").let { array ->
            List(array.length()) { index -> parseRestoreItem(array.getJSONObject(index)) }
        },
        activeSlot = json.getString("active_slot"),
        launchAfterReboot = json.getBoolean("launch_after_reboot"),
        appStarted = json.getBoolean("app_started"),
    )
}
