package com.uclone.slices.v2.desktop

internal object DesktopShortcutContract {
    const val PERMISSION = "com.uclone.slices.v2.permission.DESKTOP_SWITCH"
    const val STATE_AUTHORITY = "com.uclone.slices.v2.desktop.state"
    const val METHOD_QUERY_STATE = "query_desktop_shortcut_state"
    const val KEY_SHOW = "show"
    const val KEY_PACKAGE_NAME = "package_name"
    const val KEY_TARGET_SLOT = "target_slot"
    const val KEY_TARGET_NAME = "target_name"

    const val ACTION_QUICK_SWITCH = "com.uclone.slices.v2.action.DESKTOP_QUICK_SWITCH"
    const val EXTRA_REQUEST_ID = "request_id"
}
