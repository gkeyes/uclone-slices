package com.uclone.slices.v2.launcher.relay

internal object LauncherRelayContract {
    const val LAUNCHER_PACKAGE = "com.miui.home"
    const val LAUNCHER_VERSION_CODE = 801025341L
    const val LAUNCHER_VERSION_NAME = "RELEASE-8.01.02.5341-260807-08151903-R"

    const val RELAY_AUTHORITY = "com.uclone.slices.v2.launcher.relay"
    const val METHOD_QUERY_STATE = "query_desktop_shortcut_state"
    const val METHOD_CREATE_ACTION = "create_desktop_shortcut_action"

    const val MANAGER_PACKAGE = "com.uclone.slices.v2"
    const val MANAGER_STATE_AUTHORITY = "com.uclone.slices.v2.desktop.state"
    const val MANAGER_SERVICE =
        "com.uclone.slices.v2.desktop.DesktopQuickSwitchService"
    const val MANAGER_ACTION = "com.uclone.slices.v2.action.DESKTOP_QUICK_SWITCH"

    const val KEY_SHOW = "show"
    const val KEY_PACKAGE_NAME = "package_name"
    const val KEY_TARGET_SLOT = "target_slot"
    const val KEY_TARGET_NAME = "target_name"
    const val KEY_PENDING_INTENT = "pending_intent"
    const val KEY_REQUEST_ID = "request_id"

    const val MARKER_SHORTCUT_ID = "uclone_slices_desktop_switch"
    const val MARKER_EXTRA = "uclone_slices_marker"
    const val MARKER_VALUE = "com.uclone.slices.v2.launcher"

    const val FIXTURE_PACKAGE = "com.uclone.slices.fixture"
    const val PROBE_TITLE = "Slices Hook 探针"
}
