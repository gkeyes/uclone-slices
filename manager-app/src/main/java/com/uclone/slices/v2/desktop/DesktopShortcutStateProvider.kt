package com.uclone.slices.v2.desktop

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import android.os.Bundle

class DesktopShortcutStateProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        if (method != DesktopShortcutContract.METHOD_QUERY_STATE) return hidden()
        val packageName = extras
            ?.getString(DesktopShortcutContract.KEY_PACKAGE_NAME)
            ?.takeIf(String::isNotBlank)
            ?: return hidden()
        val state = SharedPreferencesDesktopShortcutProjection(requireNotNull(context))
            .menuState(packageName)
            ?: return hidden()
        return Bundle().apply {
            putBoolean(DesktopShortcutContract.KEY_SHOW, true)
            putString(DesktopShortcutContract.KEY_PACKAGE_NAME, state.packageName)
            putString(DesktopShortcutContract.KEY_TARGET_SLOT, state.targetSlot)
            putString(DesktopShortcutContract.KEY_TARGET_NAME, state.targetName)
        }
    }

    private fun hidden(): Bundle = Bundle().apply {
        putBoolean(DesktopShortcutContract.KEY_SHOW, false)
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0
    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0
}
