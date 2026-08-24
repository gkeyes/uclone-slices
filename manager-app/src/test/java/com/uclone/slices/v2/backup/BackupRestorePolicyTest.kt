package com.uclone.slices.v2.backup

import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class BackupRestorePolicyTest {
    private val first = "a".repeat(64)
    private val second = "b".repeat(64)

    @Test
    fun signingLineageAcceptsASharedCertificateButNeverCrossesSignerKinds() {
        val installed = SigningIdentity(SigningKind.Lineage, listOf(first, second))

        assertTrue(
            archiveSigningMatches(SigningKind.Lineage, listOf(first), installed),
        )
        assertFalse(
            archiveSigningMatches(SigningKind.Multiple, listOf(first, second), installed),
        )
    }

    @Test
    fun multipleSignersRequireTheExactCanonicalSet() {
        val installed = SigningIdentity(SigningKind.Multiple, listOf(first, second))

        assertTrue(
            archiveSigningMatches(
                SigningKind.Multiple,
                listOf(first, second),
                installed,
            ),
        )
        assertFalse(
            archiveSigningMatches(SigningKind.Multiple, listOf(first), installed),
        )
        assertFalse(
            archiveSigningMatches(
                SigningKind.Multiple,
                listOf(second, first),
                installed,
            ),
        )
    }
}
