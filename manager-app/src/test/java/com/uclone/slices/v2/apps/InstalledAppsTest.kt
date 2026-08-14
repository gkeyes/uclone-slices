package com.uclone.slices.v2.apps

import com.uclone.slices.v2.runtime.SigningKind
import java.security.MessageDigest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class InstalledAppsTest {
    @Test
    fun singleSignerHistoryKeepsAndroidVerifiedLineageOrder() {
        val oldest = byteArrayOf(1, 2, 3)
        val current = byteArrayOf(4, 5, 6)

        val identity = signingIdentityFromCertificates(
            multiple = false,
            certificates = listOf(oldest, current),
        )

        assertEquals(SigningKind.Lineage, identity?.kind)
        assertEquals(listOf(sha256(oldest), sha256(current)), identity?.sha256)
    }

    @Test
    fun multipleSignerIdentityUsesAStableCompleteSet() {
        val first = byteArrayOf(9, 8, 7)
        val second = byteArrayOf(1, 3, 5)

        val identity = signingIdentityFromCertificates(
            multiple = true,
            certificates = listOf(first, second),
        )

        assertEquals(SigningKind.Multiple, identity?.kind)
        assertEquals(listOf(sha256(first), sha256(second)).sorted(), identity?.sha256)
    }

    @Test
    fun missingOrDuplicateSignersAreRejected() {
        assertNull(signingIdentityFromCertificates(multiple = false, certificates = emptyList()))
        val signer = byteArrayOf(1, 2, 3)
        assertNull(
            signingIdentityFromCertificates(
                multiple = true,
                certificates = listOf(signer, signer),
            ),
        )
    }

    private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
        .digest(value)
        .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
}
