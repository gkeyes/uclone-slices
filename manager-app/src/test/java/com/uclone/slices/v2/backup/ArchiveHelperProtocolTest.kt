package com.uclone.slices.v2.backup

import com.uclone.slices.v2.runtime.AccountIoSource
import com.uclone.slices.v2.runtime.ArchiveScope
import com.uclone.slices.v2.runtime.SigningKind
import org.json.JSONObject
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class ArchiveHelperProtocolTest {
    @Test
    fun backupControlContainsOnlyTheDeclaredAccountSources() {
        val encoded = ArchiveHelperProtocol.encodeBackup(
            BackupArchiveRequest(
                outputPath = "/data/user/0/com.uclone.slices.v2/cache/backup-restore/out.ucsbackup",
                password = "secret".toCharArray(),
                packageName = "com.example.app",
                signingKind = SigningKind.Lineage,
                signingSha256 = listOf("a".repeat(64)),
                appVersion = "1",
                appVersionCode = 1,
                androidVersion = "17",
                device = "device",
                createdAtMillis = 1,
                scope = ArchiveScope.Account,
                activeAccountId = "base",
                launchAfterReboot = false,
                sources = listOf(
                    AccountIoSource(
                        archiveAccountId = "base",
                        slotId = "base",
                        name = "系统原始空间",
                        cePath = "/data/user/0/com.example.app",
                        dePath = "/data/user_de/0/com.example.app",
                    ),
                ),
                profile = null,
            ),
        )
        val root = JSONObject(encoded.toString(Charsets.UTF_8))

        assertEquals("backup", root.getString("op"))
        assertEquals("secret", root.getString("password"))
        assertEquals("lineage", root.getString("signing_kind"))
        assertEquals("base", root.getJSONArray("sources").getJSONObject(0).getString("kind"))
        assertEquals(1, root.getJSONArray("sources").length())
        assertEquals(false, root.has("profile"))
        assertEquals(false, root.has("app_version_code"))
    }

    @Test
    fun profiledBackupCarriesVersionForDiagnosticsAndDecodesV2Statistics() {
        val profile = BackupProfile(
            id = "delta-force-cn-account-v1",
            revision = 1,
            packageName = "com.tencent.tmgp.dfm",
            signerSha256 = listOf("a".repeat(64)),
            rulesDigest = "b".repeat(64),
            rules = listOf(
                BackupProfileRule(
                    BackupProfileDomain.Ce,
                    "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer",
                    BackupProfileRestore.Preserve,
                ),
            ),
        )
        val root = JSONObject(
            ArchiveHelperProtocol.encodeBackup(
                BackupArchiveRequest(
                    outputPath = "/tmp/out.ucsbackup",
                    password = null,
                    packageName = profile.packageName,
                    signingKind = SigningKind.Lineage,
                    signingSha256 = profile.signerSha256,
                    appVersion = "1.201.37114.81",
                    appVersionCode = 2019,
                    androidVersion = "17",
                    device = "device",
                    createdAtMillis = 1,
                    scope = ArchiveScope.Account,
                    activeAccountId = "base",
                    launchAfterReboot = false,
                    sources = emptyList(),
                    profile = profile,
                ),
            ).toString(Charsets.UTF_8),
        )

        assertEquals(2019, root.getLong("app_version_code"))
        assertEquals("preserve", root.getJSONObject("profile").getJSONArray("rules")
            .getJSONObject(0).getString("restore"))

        val decoded = assertIs<ArchiveHelperResult.Success>(
            ArchiveHelperProtocol.decode(
                """{"manifest":{"format_version":2,"package":"com.tencent.tmgp.dfm","signing_kind":"lineage","signing_sha256":["${"a".repeat(64)}"],"app_version":"1.201.37114.81","app_version_code":2019,"android_version":"17","device":"device","created_at_millis":1,"scope":"account","active_account_id":"base","launch_after_reboot":false,"logical_size":7,"accounts":[],"backup_type":"profiled_account","profile":{"id":"delta-force-cn-account-v1","revision":1,"rules_digest":"${"b".repeat(64)}","excluded_entries":42,"excluded_logical_size":8000000000}}}""",
            ),
        )
        assertEquals(2, decoded.manifest.formatVersion)
        assertEquals("profiled_account", decoded.manifest.backupType)
        assertEquals(2019, decoded.manifest.appVersionCode)
        assertEquals(8_000_000_000, decoded.manifest.profile?.excludedLogicalSize)
    }

    @Test
    fun helperManifestAndErrorsDecodeWithoutCompatibilityAliases() {
        val manifest = """
            {"manifest":{"format_version":1,"package":"com.example.app","signing_kind":"lineage","signing_sha256":["${"a".repeat(64)}"],"app_version":"1","android_version":"17","device":"device","created_at_millis":1,"scope":"account","active_account_id":"base","launch_after_reboot":false,"logical_size":7,"accounts":[{"archive_account_id":"base","name":"系统原始空间","kind":"base","source_slot":"base","ce":{"state":"data","logical_size":7,"entries":[{"path":"files/a","kind":"file","size":7,"sha256":"${"b".repeat(64)}","link_target":null,"mode":384,"mtime_seconds":1}]},"de":{"state":"empty","logical_size":0,"entries":[]}}]}}
        """.trimIndent()
        val decoded = assertIs<ArchiveHelperResult.Success>(
            ArchiveHelperProtocol.decode(manifest),
        )

        assertEquals("com.example.app", decoded.manifest.packageName)
        assertEquals(SigningKind.Lineage, decoded.manifest.signingKind)
        assertEquals(7, decoded.manifest.logicalSize)
        assertEquals(1, decoded.manifest.accounts.single().ce.entryCount)
        assertEquals(
            ArchiveHelperResult.Failure("backup_auth_failed"),
            ArchiveHelperProtocol.decode(
                """{"error":{"code":"backup_auth_failed"}}""",
            ),
        )
        assertEquals(
            ArchiveHelperResult.TransportFailure,
            ArchiveHelperProtocol.decode("{}"),
        )
    }
}
