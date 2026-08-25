package com.uclone.slices.v2.backup

import com.uclone.slices.v2.runtime.AccountIoSource
import com.uclone.slices.v2.runtime.ArchiveScope
import com.uclone.slices.v2.runtime.RestoreStaging
import com.uclone.slices.v2.runtime.SigningKind
import org.json.JSONArray
import org.json.JSONObject

internal enum class ArchiveAccountKind(val wire: String) {
    Base("base"),
    Slot("slot");

    companion object {
        fun fromWire(value: String): ArchiveAccountKind = entries.first { it.wire == value }
    }
}

internal data class ArchiveDomainSummary(
    val logicalSize: Long,
    val entryCount: Int,
)

internal data class ArchiveAccount(
    val archiveAccountId: String,
    val name: String,
    val kind: ArchiveAccountKind,
    val sourceSlot: String,
    val ce: ArchiveDomainSummary,
    val de: ArchiveDomainSummary,
)

internal data class ArchiveProfileManifest(
    val id: String,
    val revision: Int,
    val rulesDigest: String,
    val excludedEntries: Long,
    val excludedLogicalSize: Long,
)

internal data class ArchiveManifest(
    val formatVersion: Int,
    val packageName: String,
    val signingKind: SigningKind,
    val signingSha256: List<String>,
    val appVersion: String,
    val appVersionCode: Long?,
    val androidVersion: String,
    val device: String,
    val createdAtMillis: Long,
    val scope: ArchiveScope,
    val activeAccountId: String?,
    val launchAfterReboot: Boolean,
    val logicalSize: Long,
    val accounts: List<ArchiveAccount>,
    val backupType: String?,
    val profile: ArchiveProfileManifest?,
)

internal sealed interface ArchiveHelperResult {
    data class Success(val manifest: ArchiveManifest) : ArchiveHelperResult
    data class Failure(val code: String) : ArchiveHelperResult
    data object TransportFailure : ArchiveHelperResult
}

internal data class BackupArchiveRequest(
    val outputPath: String,
    val password: CharArray?,
    val packageName: String,
    val signingKind: SigningKind,
    val signingSha256: List<String>,
    val appVersion: String,
    val appVersionCode: Long,
    val androidVersion: String,
    val device: String,
    val createdAtMillis: Long,
    val scope: ArchiveScope,
    val activeAccountId: String?,
    val launchAfterReboot: Boolean,
    val sources: List<AccountIoSource>,
    val profile: BackupProfile?,
)

internal data class RestoreArchiveRequest(
    val inputPath: String,
    val password: CharArray?,
    val staging: List<RestoreStaging>,
)

internal object ArchiveHelperProtocol {
    fun encodeBackup(request: BackupArchiveRequest): ByteArray = JSONObject()
        .put("op", "backup")
        .put("output_path", request.outputPath)
        .put("password", passwordValue(request.password))
        .put("package", request.packageName)
        .put("signing_kind", request.signingKind.wire)
        .put("signing_sha256", JSONArray(request.signingSha256))
        .put("app_version", request.appVersion)
        .apply {
            request.profile?.let { profile ->
                put("app_version_code", request.appVersionCode)
                put("profile", encodeProfile(profile))
            }
        }
        .put("android_version", request.androidVersion)
        .put("device", request.device)
        .put("created_at_millis", request.createdAtMillis)
        .put("scope", request.scope.wire)
        .put("active_account_id", request.activeAccountId ?: JSONObject.NULL)
        .put("launch_after_reboot", request.launchAfterReboot)
        .put(
            "sources",
            JSONArray(
                request.sources.map { source ->
                    JSONObject()
                        .put("archive_account_id", source.archiveAccountId)
                        .put("name", source.name)
                        .put(
                            "kind",
                            if (source.slotId == "base") {
                                ArchiveAccountKind.Base.wire
                            } else {
                                ArchiveAccountKind.Slot.wire
                            },
                        )
                        .put("source_slot", source.slotId)
                        .put("ce_path", source.cePath)
                        .put("de_path", source.dePath)
                },
            ),
        )
        .toString()
        .toByteArray(Charsets.UTF_8)

    private fun encodeProfile(profile: BackupProfile): JSONObject = JSONObject()
        .put("id", profile.id)
        .put("revision", profile.revision)
        .put("rules_digest", profile.rulesDigest)
        .put(
            "rules",
            JSONArray(
                profile.rules.map { rule ->
                    JSONObject()
                        .put("domain", rule.domain.wire)
                        .put("path", rule.path)
                        .put("backup", "exclude")
                        .put("restore", rule.restore.wire)
                },
            ),
        )

    fun encodeInspect(inputPath: String, password: CharArray?): ByteArray = JSONObject()
        .put("op", "inspect")
        .put("input_path", inputPath)
        .put("password", passwordValue(password))
        .toString()
        .toByteArray(Charsets.UTF_8)

    fun encodeRestore(request: RestoreArchiveRequest): ByteArray = JSONObject()
        .put("op", "restore")
        .put("input_path", request.inputPath)
        .put("password", passwordValue(request.password))
        .put(
            "destinations",
            JSONArray(
                request.staging.map { staging ->
                    JSONObject()
                        .put("archive_account_id", staging.archiveAccountId)
                        .put("ce_path", staging.cePath)
                        .put("de_path", staging.dePath)
                },
            ),
        )
        .toString()
        .toByteArray(Charsets.UTF_8)

    fun decode(frame: String): ArchiveHelperResult = runCatching {
        val root = JSONObject(frame)
        root.optJSONObject("error")?.let { error ->
            return ArchiveHelperResult.Failure(error.getString("code"))
        }
        ArchiveHelperResult.Success(parseManifest(root.getJSONObject("manifest")))
    }.getOrElse { ArchiveHelperResult.TransportFailure }

    private fun passwordValue(password: CharArray?): Any =
        password?.concatToString() ?: JSONObject.NULL

    private fun parseManifest(json: JSONObject): ArchiveManifest = ArchiveManifest(
        formatVersion = json.getInt("format_version"),
        packageName = json.getString("package"),
        signingKind = SigningKind.entries.first {
            it.wire == json.getString("signing_kind")
        },
        signingSha256 = json.getJSONArray("signing_sha256").strings(),
        appVersion = json.getString("app_version"),
        appVersionCode = json.optLongOrNull("app_version_code"),
        androidVersion = json.getString("android_version"),
        device = json.getString("device"),
        createdAtMillis = json.getLong("created_at_millis"),
        scope = ArchiveScope.entries.first { it.wire == json.getString("scope") },
        activeAccountId = if (json.isNull("active_account_id")) {
            null
        } else {
            json.getString("active_account_id")
        },
        launchAfterReboot = json.getBoolean("launch_after_reboot"),
        logicalSize = json.getLong("logical_size"),
        accounts = json.getJSONArray("accounts").objects().map { account ->
            ArchiveAccount(
                archiveAccountId = account.getString("archive_account_id"),
                name = account.getString("name"),
                kind = ArchiveAccountKind.fromWire(account.getString("kind")),
                sourceSlot = account.getString("source_slot"),
                ce = parseDomain(account.getJSONObject("ce")),
                de = parseDomain(account.getJSONObject("de")),
            )
        },
        backupType = json.optStringOrNull("backup_type"),
        profile = json.optJSONObject("profile")?.let { profile ->
            ArchiveProfileManifest(
                id = profile.getString("id"),
                revision = profile.getInt("revision"),
                rulesDigest = profile.getString("rules_digest"),
                excludedEntries = profile.getLong("excluded_entries"),
                excludedLogicalSize = profile.getLong("excluded_logical_size"),
            )
        },
    )

    private fun parseDomain(json: JSONObject): ArchiveDomainSummary = ArchiveDomainSummary(
        logicalSize = json.getLong("logical_size"),
        entryCount = json.getJSONArray("entries").length(),
    )

    private fun JSONArray.strings(): List<String> =
        List(length()) { index -> getString(index) }

    private fun JSONArray.objects(): List<JSONObject> =
        List(length()) { index -> getJSONObject(index) }

    private fun JSONObject.optLongOrNull(name: String): Long? =
        if (has(name) && !isNull(name)) getLong(name) else null

    private fun JSONObject.optStringOrNull(name: String): String? =
        if (has(name) && !isNull(name)) getString(name) else null
}
