#!/system/bin/sh

set -u
umask 077

case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 1 ;;
esac
PROFILE_FILE=$SCRIPT_DIR/target-profile.sh
[ -f "$PROFILE_FILE" ] && [ ! -L "$PROFILE_FILE" ] || exit 1
. "$PROFILE_FILE"
RUNTIME_ROOT=$UCLONE_RUNTIME_ROOT
PACKAGE=$UCLONE_TARGET_PACKAGE
USER_ID=$UCLONE_TARGET_USER
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd
AM_BIN=/system/bin/am

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
    for path in \
        "$RUNTIME_ROOT/enrollment/packages/$PACKAGE" \
        "$RUNTIME_ROOT/catalog/packages/$PACKAGE" \
        "$RUNTIME_ROOT/registry/packages/$PACKAGE" \
        "$RUNTIME_ROOT/package-state/packages/$PACKAGE" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts/$PACKAGE" \
        "$RUNTIME_ROOT/rescue-journal/packages/$PACKAGE" \
        "$RUNTIME_ROOT/state/$PACKAGE.gate" \
        "$RUNTIME_ROOT/state/.$PACKAGE.gate.retiring" \
        "$RUNTIME_ROOT/state/.$PACKAGE.gate.retired" \
        "/data/misc_ce/$USER_ID/$UCLONE_MODULE/slots/$PACKAGE" \
        "/data/misc_de/$USER_ID/$UCLONE_MODULE/slots/$PACKAGE"
    do
        if [ -e "$path" ] || [ -L "$path" ]; then
            return 0
        fi
    done
    return 1
}

package_is_disabled() {
    disabled_packages="$("$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" ||
        return 1
    printf '%s\n' "$disabled_packages" |
        "$TOYBOX_BIN" grep -F -x "package:$PACKAGE" >/dev/null 2>&1
}

package_is_quiesced() {
    processes="$("$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    printf '%s\n' "$processes" |
        "$TOYBOX_BIN" grep -E -q "$UCLONE_TARGET_PROCESS_REGEX"
    status=$?
    [ "$status" -eq 1 ]
}

contain_once() {
    safe_binary "$TOYBOX_BIN" || return 1
    safe_binary "$CMD_BIN" || return 1
    safe_binary "$AM_BIN" || return 1
    "$CMD_BIN" package disable-user --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1 || return 1
    "$AM_BIN" force-stop --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1 || return 1
    package_is_disabled && package_is_quiesced
}

attempt_once() {
    if ! management_artifact_present; then
        printf '%s\n' not-managed
        return 0
    fi
    if contain_once; then
        printf '%s\n' held
        return 0
    fi
    printf '%s\n' recovery-required
    return 1
}

case "${1:-}" in
    --once)
        [ -z "${2:-}" ] || exit 2
        attempt_once
        ;;
    --watch)
        [ -z "${2:-}" ] || exit 2
        while management_artifact_present; do
            contain_once || :
            if safe_binary "$TOYBOX_BIN"; then
                "$TOYBOX_BIN" sleep 2
            else
                /system/bin/sleep 2 2>/dev/null || exit 1
            fi
        done
        exit 0
        ;;
    *)
        exit 2
        ;;
esac
