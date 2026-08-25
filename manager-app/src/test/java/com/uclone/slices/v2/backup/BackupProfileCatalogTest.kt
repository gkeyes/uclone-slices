package com.uclone.slices.v2.backup

import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class BackupProfileCatalogTest {
    private val official = "1".repeat(64)

    @Test
    fun packagedDeltaForceProfileMatchesTheVerifiedOfficialSigner() {
        val document = requireNotNull(
            javaClass.getResourceAsStream(
                "/backup_profiles/com.tencent.tmgp.dfm-v1.json",
            ),
        ).use { it.readBytes() }
        val catalog = BackupProfileCatalog.fromJson(listOf(document))

        val profile = catalog.resolve(
            "com.tencent.tmgp.dfm",
            SigningIdentity(
                SigningKind.Lineage,
                listOf("1985a9bd670ad5d1251ee34d1fa1eb3a5f393dd91a8968048b46d4a0241de49b"),
            ),
        )

        assertEquals("delta-force-cn-account-v1", profile?.id)
        assertEquals(5, profile?.rules?.size)
        assertEquals(4, profile?.rules?.count { it.restore == BackupProfileRestore.Preserve })
    }

    @Test
    fun packageAndOfficialSignerSelectProfileWithoutAppVersionGate() {
        val catalog = BackupProfileCatalog.fromJson(listOf(profileJson().toByteArray()))
        val signing = SigningIdentity(SigningKind.Lineage, listOf("0".repeat(64), official))

        val profile = catalog.resolve("com.tencent.tmgp.dfm", signing)

        assertEquals("delta-force-cn-account-v1", profile?.id)
        assertEquals(1, profile?.revision)
        assertTrue(profile?.rules?.any {
            it.path == "files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/*/Paks"
        } == true)
        assertNull(catalog.resolve("com.example.other", signing))
        assertNull(
            catalog.resolve(
                "com.tencent.tmgp.dfm",
                SigningIdentity(SigningKind.Lineage, listOf("2".repeat(64))),
            ),
        )
    }

    @Test
    fun archivedProfileRequiresTheExactHistoricalRevisionAndDigest() {
        val catalog = BackupProfileCatalog.fromJson(listOf(profileJson().toByteArray()))
        val signing = SigningIdentity(SigningKind.Lineage, listOf(official))
        val current = requireNotNull(catalog.resolve("com.tencent.tmgp.dfm", signing))
        val archived = ArchiveProfileManifest(
            id = current.id,
            revision = current.revision,
            rulesDigest = current.rulesDigest,
            excludedEntries = 1,
            excludedLogicalSize = 2,
        )

        assertEquals(current, catalog.resolve(current.packageName, archived, signing))
        assertNull(
            catalog.resolve(
                current.packageName,
                archived.copy(rulesDigest = "f".repeat(64)),
                signing,
            ),
        )
    }

    @Test
    fun unsafeOrOverlappingProfileFailsClosed() {
        val unsafe = profileJson(
            rules = """[
              {"domain":"ce","path":"../files","backup":"exclude","restore":"preserve"}
            ]""",
        )
        val overlapping = profileJson(
            rules = """[
              {"domain":"ce","path":"files/game","backup":"exclude","restore":"preserve"},
              {"domain":"ce","path":"files/game/Paks","backup":"exclude","restore":"preserve"}
            ]""",
        )
        val catalog = BackupProfileCatalog.fromJson(
            listOf(unsafe.toByteArray(), overlapping.toByteArray()),
        )

        assertNull(
            catalog.resolve(
                "com.tencent.tmgp.dfm",
                SigningIdentity(SigningKind.Lineage, listOf(official)),
            ),
        )
    }

    private fun profileJson(
        rules: String = """[
          {"domain":"ce","path":"files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer","backup":"exclude","restore":"preserve"},
          {"domain":"ce","path":"files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/*/Paks","backup":"exclude","restore":"preserve"}
        ]""",
    ): String = """{
      "schema_version":1,
      "id":"delta-force-cn-account-v1",
      "revision":1,
      "package":"com.tencent.tmgp.dfm",
      "signer_sha256":["$official"],
      "rules":$rules
    }"""
}
