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
PACKAGE=
USER_ID=$UCLONE_TARGET_USER
SLOTCTL_BIN="$SCRIPT_DIR/bin/slotctl"
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd
AM_BIN=/system/bin/am
LOG_DIR="$RUNTIME_ROOT/logs"

safe_binary() {
    binary="$1"
    [ -f "$binary" ] || return 1
    [ ! -L "$binary" ] || return 1
    [ -x "$binary" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$binary" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$("$TOYBOX_BIN" stat -c '%a' "$binary" 2>/dev/null)" || return 1
    case "$mode" in
        *[!0-7]*|'') return 1 ;;
    esac
    other=$((mode % 10))
    group=$(((mode / 10) % 10))
    [ $((other & 2)) -eq 0 ] && [ $((group & 2)) -eq 0 ]
}

valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" |
        "$TOYBOX_BIN" grep -E -x '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' \
            >/dev/null 2>&1
}

package_is_disabled() {
    safe_binary "$TOYBOX_BIN" || return 1
    disabled_packages="$("$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" ||
        return 1
    printf '%s\n' "$disabled_packages" |
        "$TOYBOX_BIN" grep -F -x "package:$PACKAGE" >/dev/null 2>&1
}

package_is_quiesced() {
    safe_binary "$TOYBOX_BIN" || return 1
    processes="$("$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    while IFS= read -r process; do
        case "$process" in
            "$PACKAGE"|"$PACKAGE":*) return 1 ;;
        esac
    done <<EOF
$processes
EOF
    return 0
}

contain_package() {
    disable_status=0
    force_stop_status=0
    "$CMD_BIN" package disable-user --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1
    disable_status=$?
    "$AM_BIN" force-stop --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1
    force_stop_status=$?
    [ "$disable_status" -eq 0 ] || return 1
    [ "$force_stop_status" -eq 0 ] || return 1
    package_is_disabled || return 1
    package_is_quiesced || return 1
}

log() {
    message="$1"
    [ -L "$RUNTIME_ROOT" ] && return 0
    [ -d "$RUNTIME_ROOT" ] || return 0
    [ -L "$LOG_DIR" ] && return 0
    [ -d "$LOG_DIR" ] || return 0
    [ -L "$LOG_DIR/rescue.log" ] && return 0
    [ -e "$LOG_DIR/rescue.log" ] && [ ! -f "$LOG_DIR/rescue.log" ] && return 0
    printf '%s\n' "$message" >>"$LOG_DIR/rescue.log" 2>/dev/null
}

fail_closed() {
    reason="$1"
    if contain_package; then
        log "RECOVERY_REQUIRED: $reason; containment proved"
        printf '%s\n' "UClone Slots rescue: $reason; package disabled and quiesced."
    else
        log "RECOVERY_REQUIRED: $reason; containment could not be proved"
        printf '%s\n' "UClone Slots rescue: $reason; package containment could not be proved."
    fi
    exit 1
}

first_arg="${1:-}"
second_arg="${2:-}"
if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
    [ "$second_arg" = "--to-base" ] && [ -z "${3:-}" ] || exit 2
    safe_binary "$TOYBOX_BIN" || exit 1
    valid_package "$first_arg" || exit 2
    PACKAGE=$first_arg
elif [ "$first_arg" = "$UCLONE_TARGET_PACKAGE" ] &&
    [ "$second_arg" = "--to-base" ] && [ -z "${3:-}" ]; then
    PACKAGE=$UCLONE_TARGET_PACKAGE
elif [ "$first_arg" = "--to-base" ] && [ -z "$second_arg" ] && [ -z "${3:-}" ]; then
    PACKAGE=$UCLONE_TARGET_PACKAGE
else
    exit 2
fi

if [ -L "$RUNTIME_ROOT" ] || [ ! -d "$RUNTIME_ROOT" ]; then
    fail_closed "runtime root is unavailable or unsafe"
fi
if [ -L "$LOG_DIR" ] || [ ! -d "$LOG_DIR" ]; then
    fail_closed "runtime log directory is unavailable or unsafe"
fi
if ! safe_binary "$TOYBOX_BIN"; then
    fail_closed "trusted toybox is unavailable"
fi
if ! safe_binary "$SLOTCTL_BIN"; then
    fail_closed "trusted rescue runtime is unavailable"
fi

if "$TOYBOX_BIN" nsenter -t 1 -m -- "$SLOTCTL_BIN" rescue "$PACKAGE" --to-base >>"$LOG_DIR/rescue.log" 2>&1; then
    log "OK: slotctl proved base rescue and exact package state restoration"
    printf '%s\n' "UClone Slots rescue: base view restored; exact package state restored."
    exit 0
fi

fail_closed "slotctl could not prove base rescue"
