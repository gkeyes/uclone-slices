package com.uclone.slices.v2.gate

import android.content.Intent

/**
 * Which launch the preparation page re-issues. The hook hands over the launch it replaced so a
 * share, deep link or notification reaches the screen it named; it is re-issued only when it
 * names the package Root prepared, otherwise the App's home screen is opened instead.
 */
internal object OriginalLaunch {
    /** Result forwarding and URI grants belonged to the original caller and cannot be passed on. */
    private const val DROPPED_FLAGS = Intent.FLAG_ACTIVITY_FORWARD_RESULT or
        Intent.FLAG_GRANT_READ_URI_PERMISSION or
        Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
        Intent.FLAG_GRANT_PREFIX_URI_PERMISSION

    fun usable(originalPackage: String?, preparedPackage: String): Boolean =
        originalPackage != null && originalPackage == preparedPackage

    fun flags(original: Int): Int =
        (original and DROPPED_FLAGS.inv()) or Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION
}
