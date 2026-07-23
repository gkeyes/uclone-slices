#!/system/bin/sh
set -u
umask 077
case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 1 ;;
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
RUNTIME_BIN=$SCRIPT_DIR/bin/ucloned
SLOTCTL_BIN=$SCRIPT_DIR/bin/slotctl
RESCUE_PACKAGES=$SCRIPT_DIR/rescue-retired-packages.sh
COMMAND_TIMEOUT_SECONDS=2
HELPER_TIMEOUT_SECONDS=4
RUNTIME_TIMEOUT_SECONDS=8
CONTAINMENT_REQUEST=$RUNTIME_ROOT/state/emergency-containment.request
safe_binary() {
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
management_artifact_present() {
    if [ -L "$RUNTIME_ROOT" ] || { [ -e "$RUNTIME_ROOT" ] && [ ! -d "$RUNTIME_ROOT" ]; }; then
        return 0
    fi
    [ -d "$RUNTIME_ROOT" ] || return 1
    safe_binary "$RESCUE_PACKAGES" || return 0
    active_control="$($TOYBOX_BIN timeout -s 9 "$HELPER_TIMEOUT_SECONDS" \
        "$RESCUE_PACKAGES" --active-control 2>/dev/null)" || return 0
    [ -n "$active_control" ]
}
runtime_owns_containment() {
    safe_binary "$SLOTCTL_BIN" || return 1
    "$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$SLOTCTL_BIN" probe >/dev/null 2>&1
}
retire_containment_request() {
    if [ ! -e "$CONTAINMENT_REQUEST" ] && [ ! -L "$CONTAINMENT_REQUEST" ]; then
        return 0
    fi
    [ -f "$CONTAINMENT_REQUEST" ] && [ ! -L "$CONTAINMENT_REQUEST" ] || return 1
    "$TOYBOX_BIN" rm -f "$CONTAINMENT_REQUEST" >/dev/null 2>&1 || return 1
    [ ! -e "$CONTAINMENT_REQUEST" ] && [ ! -L "$CONTAINMENT_REQUEST" ]
}
valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" |
        "$TOYBOX_BIN" grep -E -x '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' \
            >/dev/null 2>&1
}
discover_packages() {
    "$TOYBOX_BIN" timeout -s 9 "$HELPER_TIMEOUT_SECONDS" \
        "$RESCUE_PACKAGES" --active-control || return 1
}
package_is_disabled() {
    package="$1"
    disabled_packages="$("$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" ||
        return 1
    printf '%s\n' "$disabled_packages" |
        "$TOYBOX_BIN" grep -F -x "package:$package" >/dev/null 2>&1
}
package_is_quiesced() {
    package="$1"
    processes="$("$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    while IFS= read -r process; do
        case "$process" in "$package"|"$package":*) return 1 ;; esac
    done <<EOF
$processes
EOF
    return 0
}
contain_once() {
    package="$1"
    safe_binary "$TOYBOX_BIN" || return 1
    safe_binary "$CMD_BIN" || return 1
    safe_binary "$AM_BIN" || return 1
    "$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$CMD_BIN" package disable-user --user "$USER_ID" "$package" \
        >/dev/null 2>&1 || return 1
    "$TOYBOX_BIN" timeout -s 9 "$COMMAND_TIMEOUT_SECONDS" \
        "$AM_BIN" force-stop --user "$USER_ID" "$package" \
        >/dev/null 2>&1 || return 1
    package_is_disabled "$package" && package_is_quiesced "$package"
}
contain_discovered() {
    packages="$(discover_packages)"
    status=$?
    packages="$(printf '%s\n' "$packages" | "$TOYBOX_BIN" sort -u)" || return 1
    result=0
    while IFS= read -r package; do
        [ -n "$package" ] || continue
        contain_once "$package" || result=1
    done <<EOF
$packages
EOF
    [ "$result" -eq 0 ] || return 1
    [ "$status" -eq 0 ] || return 1
    [ -n "$packages" ] || return 2
}
contain_with_runtime() {
    safe_binary "$RUNTIME_BIN" || return 1
    result="$("$TOYBOX_BIN" timeout -s 9 "$RUNTIME_TIMEOUT_SECONDS" \
        "$TOYBOX_BIN" nsenter -t 1 -m -- \
        "$RUNTIME_BIN" --startup-gate 2>/dev/null)" || return 1
    case "$result" in held) return 0 ;; not-managed) return 2 ;; *) return 1 ;; esac
}
attempt_once() {
    if ! management_artifact_present; then
        printf '%s\n' not-managed
        return 0
    fi
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        contain_with_runtime
        runtime_status=$?
        case "$runtime_status" in
            0) ;;
            2) printf '%s\n' not-managed; return 0 ;;
            *)
                contain_discovered
                fallback_status=$?
                case "$fallback_status" in
                    0) ;;
                    2) printf '%s\n' not-managed; return 0 ;;
                    *) printf '%s\n' recovery-required; return 1 ;;
                esac
                ;;
        esac
    elif ! contain_once "$PACKAGE"; then
        printf '%s\n' recovery-required
        return 1
    fi
    if management_artifact_present; then
        printf '%s\n' held
        return 0
    fi
    printf '%s\n' not-managed
    return 0
}
case "${1:-}" in
    --once) [ -z "${2:-}" ] || exit 2; attempt_once ;;
    --watch)
        [ -z "${2:-}" ] || exit 2
        while :; do
            if runtime_owns_containment && retire_containment_request; then
                exit 0
            fi
            result="$(attempt_once 2>/dev/null)"
            status=$?
            if [ "$status" -eq 0 ] && [ "$result" = not-managed ]; then
                if [ -f "$CONTAINMENT_REQUEST" ] && [ ! -L "$CONTAINMENT_REQUEST" ]; then
                    "$TOYBOX_BIN" rm -f "$CONTAINMENT_REQUEST" >/dev/null 2>&1 || :
                fi
                exit 0
            fi
            if safe_binary "$TOYBOX_BIN"; then
                "$TOYBOX_BIN" sleep 2
            else
                /system/bin/sleep 2 2>/dev/null || exit 1
            fi
        done
        ;;
    *)
        exit 2
        ;;
esac
