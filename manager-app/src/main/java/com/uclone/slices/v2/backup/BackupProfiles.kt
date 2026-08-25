package com.uclone.slices.v2.backup

import android.content.Context
import com.uclone.slices.v2.runtime.SigningIdentity
import java.security.MessageDigest
import org.json.JSONObject

internal enum class BackupProfileDomain(val wire: String) {
    Ce("ce"),
    De("de");

    companion object {
        fun fromWire(value: String): BackupProfileDomain = entries.first { it.wire == value }
    }
}

internal enum class BackupProfileRestore(val wire: String) {
    Preserve("preserve"),
    Discard("discard");

    companion object {
        fun fromWire(value: String): BackupProfileRestore = entries.first { it.wire == value }
    }
}

internal data class BackupProfileRule(
    val domain: BackupProfileDomain,
    val path: String,
    val restore: BackupProfileRestore,
)

internal data class BackupProfile(
    val id: String,
    val revision: Int,
    val packageName: String,
    val signerSha256: List<String>,
    val rulesDigest: String,
    val rules: List<BackupProfileRule>,
)

internal class BackupProfileCatalog private constructor(
    private val profiles: List<BackupProfile>,
) {
    fun resolve(packageName: String, signing: SigningIdentity?): BackupProfile? = profiles
        .asSequence()
        .filter { it.packageName == packageName && it.matches(signing) }
        .maxByOrNull(BackupProfile::revision)

    fun resolve(
        packageName: String,
        archived: ArchiveProfileManifest?,
        signing: SigningIdentity?,
    ): BackupProfile? {
        if (archived == null) return null
        return profiles.singleOrNull {
            it.packageName == packageName &&
                it.id == archived.id &&
                it.revision == archived.revision &&
                it.rulesDigest == archived.rulesDigest &&
                it.matches(signing)
        }
    }

    private fun BackupProfile.matches(signing: SigningIdentity?): Boolean =
        signing != null && signerSha256.any(signing.sha256::contains)

    companion object {
        fun load(context: Context): BackupProfileCatalog = runCatching {
            val assets = context.assets
            val documents = assets.list(ASSET_DIRECTORY)
                .orEmpty()
                .filter { it.endsWith(".json") }
                .sorted()
                .mapNotNull { name ->
                    runCatching {
                        assets.open("$ASSET_DIRECTORY/$name").use { it.readBytes() }
                    }.getOrNull()
                }
            fromJson(documents)
        }.getOrElse { BackupProfileCatalog(emptyList()) }

        internal fun fromJson(documents: List<ByteArray>): BackupProfileCatalog {
            val parsed = documents.mapNotNull { document ->
                runCatching { parse(document) }.getOrNull()
            }
            val unique = parsed
                .groupBy { it.id to it.revision }
                .values
                .filter { it.size == 1 }
                .flatten()
            return BackupProfileCatalog(unique)
        }

        private fun parse(document: ByteArray): BackupProfile {
            val json = JSONObject(document.toString(Charsets.UTF_8))
            require(json.getInt("schema_version") == 1)
            val id = json.getString("id")
            val revision = json.getInt("revision")
            val packageName = json.getString("package")
            require(id.matches(Regex("[a-z0-9._-]{1,128}")))
            require(revision > 0)
            require(packageName.matches(Regex("[A-Za-z0-9_]+(?:\\.[A-Za-z0-9_]+)+")))
            val signers = json.getJSONArray("signer_sha256").let { values ->
                List(values.length()) { index -> values.getString(index) }
            }
            require(signers.isNotEmpty())
            require(signers.distinct().size == signers.size)
            require(signers.all { it.matches(Regex("[0-9a-f]{64}")) })
            val rules = json.getJSONArray("rules").let { values ->
                List(values.length()) { index ->
                    val rule = values.getJSONObject(index)
                    require(rule.getString("backup") == "exclude")
                    BackupProfileRule(
                        domain = BackupProfileDomain.fromWire(rule.getString("domain")),
                        path = rule.getString("path"),
                        restore = BackupProfileRestore.fromWire(rule.getString("restore")),
                    )
                }
            }
            require(rules.isNotEmpty() && rules.size <= 64)
            rules.forEach { validatePath(it.path) }
            require(rules.distinctBy { it.domain to it.path }.size == rules.size)
            rules.forEachIndexed { index, left ->
                rules.drop(index + 1).forEach { right ->
                    require(left.domain != right.domain || !pathsOverlap(left.path, right.path))
                }
            }
            return BackupProfile(
                id = id,
                revision = revision,
                packageName = packageName,
                signerSha256 = signers,
                rulesDigest = sha256(document),
                rules = rules,
            )
        }

        private fun validatePath(path: String) {
            require(path.isNotEmpty() && path.length <= 4096 && !path.startsWith('/'))
            require(path.split('/').all { segment ->
                segment.isNotEmpty() &&
                    segment !in setOf(".", "..", "**") &&
                    (segment == "*" || segment.all {
                        it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' ||
                            it == '.' || it == '_' || it == '-'
                    })
            })
        }

        private fun pathsOverlap(left: String, right: String): Boolean {
            val leftParts = left.split('/')
            val rightParts = right.split('/')
            return leftParts.zip(rightParts).all { (leftPart, rightPart) ->
                leftPart == rightPart || leftPart == "*" || rightPart == "*"
            }
        }

        private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
            .digest(value)
            .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }

        private const val ASSET_DIRECTORY = "backup_profiles"
    }
}
