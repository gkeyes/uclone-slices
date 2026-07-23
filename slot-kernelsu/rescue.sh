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
SYSTEM_SHELL=/system/bin/sh
FD_ROOT=/proc/self/fd
FD_EXEC_INTERPRETER=
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

file_digest() {
    line="$($TOYBOX_BIN sha256sum "$1" 2>/dev/null)" || return 1
    digest=${line%% *}
    [ "${#digest}" -eq 64 ] || return 1
    case "$digest" in *[!0-9a-f]*|'') return 1 ;; esac
    printf '%s\n' "$digest"
}

pid1_mount_namespace() {
    current="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/self/ns/mnt 2>/dev/null)" || return 1
    pid1="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/1/ns/mnt 2>/dev/null)" || return 1
    case "$current" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    case "$pid1" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "$current" = "$pid1" ]
}

exec_trusted_binary() {
    binary=$1
    shift
    safe_binary "$binary" || return 1
    exec 9<"$binary" || return 1
    descriptor=$FD_ROOT/9
    [ -f "$descriptor" ] && [ -x "$descriptor" ] || return 1
    owner="$($TOYBOX_BIN stat -L -c '%u:%g' "$descriptor" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$($TOYBOX_BIN stat -L -c '%a' "$descriptor" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $((mode % 10 & 2)) -eq 0 ] && [ $(((mode / 10) % 10 & 2)) -eq 0 ] || return 1
    path_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$binary" 2>/dev/null)" || return 1
    descriptor_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$descriptor" 2>/dev/null)" || return 1
    case "$descriptor_node" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "$path_node" = "$descriptor_node" ] || return 1
    path_digest="$(file_digest "$binary")" || return 1
    descriptor_digest="$(file_digest "$descriptor")" || return 1
    [ "$path_digest" = "$descriptor_digest" ] || return 1
    if [ -n "$FD_EXEC_INTERPRETER" ]; then
        exec "$FD_EXEC_INTERPRETER" "$descriptor" "$@"
    fi
    exec "$descriptor" "$@"
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

if [ "${1:-}" = --pid1-slotctl ]; then
    [ "$#" -eq 2 ] || exit 2
    inner_package=$2
    safe_binary "$TOYBOX_BIN" || exit 1
    pid1_mount_namespace || exit 1
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        valid_package "$inner_package" || exit 2
    else
        [ "$inner_package" = "$UCLONE_TARGET_PACKAGE" ] || exit 2
    fi
    exec_trusted_binary "$SLOTCTL_BIN" rescue "$inner_package" --to-base || exit 1
fi

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
if "$TOYBOX_BIN" nsenter -t 1 -m -- "$SYSTEM_SHELL" "$SCRIPT_DIR/rescue.sh" \
    --pid1-slotctl "$PACKAGE" >>"$LOG_DIR/rescue.log" 2>&1; then
    log "OK: slotctl proved base rescue and exact package state restoration"
    printf '%s\n' "UClone Slots rescue: base view restored; exact package state restored."
    exit 0
fi

fail_closed "slotctl could not prove base rescue"
