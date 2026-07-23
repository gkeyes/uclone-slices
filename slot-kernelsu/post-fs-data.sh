#!/system/bin/sh
set -u
umask 077
case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 0 ;;
esac
PROFILE_FILE=$SCRIPT_DIR/target-profile.sh
PROFILE_LOADER=$SCRIPT_DIR/profile-loader.sh
UCLONE_TARGET_PROFILE=generic
UCLONE_TARGET_PACKAGE=com.uclone.slots.preview
UCLONE_TARGET_USER=0
UCLONE_MODULE=uclone-slices-preview
UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview
if [ -f "$PROFILE_LOADER" ] && [ ! -L "$PROFILE_LOADER" ]; then
    . "$PROFILE_LOADER"
    uclone_load_profile "$PROFILE_FILE" || :
fi
RUNTIME_ROOT=$UCLONE_RUNTIME_ROOT
PACKAGE=$UCLONE_TARGET_PACKAGE
USER_ID=$UCLONE_TARGET_USER
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd
AM_BIN=/system/bin/am
STARTUP_GATE=$SCRIPT_DIR/startup-gate.sh
EMERGENCY_CONTAINMENT=$SCRIPT_DIR/emergency-containment.sh
RESCUE_PACKAGES=$SCRIPT_DIR/rescue-retired-packages.sh
POST_FS_SETUP=$SCRIPT_DIR/post-fs-setup.sh
POST_FS_HOOK_BUDGET_SECONDS=20
POST_FS_SETUP_TIMEOUT_SECONDS=8
POST_FS_CONTAINMENT_TIMEOUT_SECONDS=8
COMMAND_TIMEOUT_SECONDS=2
HELPER_TIMEOUT_SECONDS=4
CONTAINMENT_REQUEST=$RUNTIME_ROOT/state/emergency-containment.request
discover_generic_packages() {
    safe_module_binary "$RESCUE_PACKAGES" || return 1
    "$TOYBOX_BIN" timeout -s 9 "$HELPER_TIMEOUT_SECONDS" \
        "$RESCUE_PACKAGES" --active-control || return 1
}
contain_builtin_once() {
    safe_module_binary "$CMD_BIN" || return 1
    safe_module_binary "$AM_BIN" || return 1
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        packages="$(discover_generic_packages)"
        discovery_status=$?
        packages="$(printf '%s\n' "$packages" | "$TOYBOX_BIN" sort -u)" || return 1
    else
        packages=$PACKAGE
        discovery_status=0
    fi
    [ -n "$packages" ] || return 1
    result=0
    while IFS= read -r package; do
        [ -n "$package" ] || continue
        "$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
            "$CMD_BIN" package disable-user --user "$USER_ID" "$package" \
            >/dev/null 2>&1 || result=1
        "$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
            "$AM_BIN" force-stop --user "$USER_ID" "$package" \
            >/dev/null 2>&1 || result=1
        package_is_disabled "$package" || result=1
        package_is_quiesced "$package" || result=1
    done <<EOF
$packages
EOF
    [ "$result" -eq 0 ] && [ "$discovery_status" -eq 0 ]
}
package_is_disabled() {
    disabled="$("$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" || return 1
    printf '%s\n' "$disabled" |
        "$TOYBOX_BIN" grep -F -x "package:$1" >/dev/null 2>&1
}
package_is_quiesced() {
    processes="$("$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    while IFS= read -r process; do
        case "$process" in "$1"|"$1":*) return 1 ;; esac
    done <<EOF
$processes
EOF
    return 0
}
launch_builtin_containment() {
    (
        while management_artifact_present; do
            contain_builtin_once || :
            "$TOYBOX_BIN" sleep 2 2>/dev/null || /system/bin/sleep 2 2>/dev/null || exit 1
        done
    ) &
    BUILTIN_WATCHER_PID=$!
    return 0
}
management_artifact_present() {
    if [ -L "$RUNTIME_ROOT" ] || { [ -e "$RUNTIME_ROOT" ] && [ ! -d "$RUNTIME_ROOT" ]; }; then
        return 0
    fi
    [ -d "$RUNTIME_ROOT" ] || return 1
    safe_module_binary "$RESCUE_PACKAGES" || return 0
    active_control="$($TOYBOX_BIN timeout -s 9 "$HELPER_TIMEOUT_SECONDS" \
        "$RESCUE_PACKAGES" --active-control 2>/dev/null)" || return 0
    [ -n "$active_control" ]
}
write_containment_request() {
    watcher_pid="$1"
    request_dir=${CONTAINMENT_REQUEST%/*}
    if [ -L "$RUNTIME_ROOT" ] || { [ -e "$RUNTIME_ROOT" ] && [ ! -d "$RUNTIME_ROOT" ]; }; then
        return 1
    fi
    "$TOYBOX_BIN" mkdir -p "$request_dir" >/dev/null 2>&1 || return 1
    [ -d "$request_dir" ] && [ ! -L "$request_dir" ] || return 1
    request_tmp=$CONTAINMENT_REQUEST.$$
    if ! printf 'state=pending\nwatcher_pid=%s\nhook_budget_seconds=%s\n' \
        "$watcher_pid" "$POST_FS_HOOK_BUDGET_SECONDS" >"$request_tmp"; then
        return 1
    fi
    "$TOYBOX_BIN" chmod 600 "$request_tmp" >/dev/null 2>&1 || return 1
    "$TOYBOX_BIN" mv -f "$request_tmp" "$CONTAINMENT_REQUEST" >/dev/null 2>&1
}
launch_emergency_containment() {
    status=0
    managed=0
    management_artifact_present && managed=1
    if safe_toybox && safe_module_binary "$EMERGENCY_CONTAINMENT"; then
        ("$EMERGENCY_CONTAINMENT" --watch >/dev/null 2>&1) &
        watcher_pid=$!
        [ "$managed" -eq 0 ] || write_containment_request "$watcher_pid" || status=1
        "$TOYBOX_BIN" timeout -s 9 "$POST_FS_CONTAINMENT_TIMEOUT_SECONDS" \
            "$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1 || status=1
    elif [ "$managed" -eq 1 ]; then
        launch_builtin_containment || status=1
        write_containment_request "$BUILTIN_WATCHER_PID" || status=1
    fi
    return "$status"
}
fail_closed() {
    launch_emergency_containment || :
    exit 0
}
safe_toybox() {
    [ -f "$TOYBOX_BIN" ] || return 1
    [ ! -L "$TOYBOX_BIN" ] || return 1
    [ -x "$TOYBOX_BIN" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    case "$owner" in 0:*) return 0 ;; *) return 1 ;; esac
}
safe_module_binary() {
    binary="$1"
    [ -f "$binary" ] || return 1
    [ ! -L "$binary" ] || return 1
    [ -x "$binary" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$binary" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$("$TOYBOX_BIN" stat -c '%a' "$binary" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}
safe_toybox || fail_closed
safe_module_binary "$EMERGENCY_CONTAINMENT" || fail_closed
safe_module_binary "$STARTUP_GATE" || fail_closed
safe_module_binary "$POST_FS_SETUP" || fail_closed
if [ $((POST_FS_SETUP_TIMEOUT_SECONDS + POST_FS_CONTAINMENT_TIMEOUT_SECONDS)) \
    -gt "$POST_FS_HOOK_BUDGET_SECONDS" ]; then
    fail_closed
fi
"$TOYBOX_BIN" timeout -s 9 "$POST_FS_SETUP_TIMEOUT_SECONDS" \
    "$POST_FS_SETUP" || fail_closed
launch_emergency_containment || exit 0
    ("$STARTUP_GATE" >/dev/null 2>&1) &
exit 0
