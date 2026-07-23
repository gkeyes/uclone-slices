#!/system/bin/sh

set -u
umask 077

INSTALLED_MODULE=/data/adb/modules/uclone-slices-preview
TOYBOX_BIN=/system/bin/toybox
SYSTEM_SHELL=/system/bin/sh
PROC_ROOT=/proc
DAEMON_NAME=ucloned
DAEMON_BINARY=$INSTALLED_MODULE/bin/ucloned

fail() {
    printf 'UClone Slots daemon freeze: %s\n' "$1" >&2
    exit 1
}

safe_toybox() {
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || return 1
    [ "$($TOYBOX_BIN stat -c '%u' "$TOYBOX_BIN" 2>/dev/null)" = 0 ]
}

safe_daemon_binary() {
    [ -f "$DAEMON_BINARY" ] && [ ! -L "$DAEMON_BINARY" ] && [ -x "$DAEMON_BINARY" ] || return 1
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$DAEMON_BINARY" 2>/dev/null)" || return 1
    case "$metadata" in 0:0:700|0:0:500) return 0 ;; *) return 1 ;; esac
}

pid1_mount_namespace() {
    current="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/self/ns/mnt 2>/dev/null)" || return 1
    pid1="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/1/ns/mnt 2>/dev/null)" || return 1
    case "$current" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    case "$pid1" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "$current" = "$pid1" ]
}

file_digest() {
    line="$($TOYBOX_BIN sha256sum "$1" 2>/dev/null)" || return 1
    digest=${line%% *}
    [ "${#digest}" -eq 64 ] || return 1
    case "$digest" in *[!0-9a-f]*|'') return 1 ;; esac
    printf '%s\n' "$digest"
}

read_process() {
    pid=$1
    case "$pid" in *[!0-9]*|'') return 1 ;; esac
    process_root=$PROC_ROOT/$pid
    [ -d "$process_root" ] && [ ! -L "$process_root" ] || return 1
    [ "$($TOYBOX_BIN stat -c '%u' "$process_root" 2>/dev/null)" = 0 ] || return 1
    executable="$($TOYBOX_BIN readlink "$process_root/exe" 2>/dev/null)" || return 1
    [ "$executable" = "$DAEMON_BINARY" ] || return 1
    process_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$process_root/exe" 2>/dev/null)" || return 1
    installed_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$DAEMON_BINARY" 2>/dev/null)" || return 1
    [ "$process_node" = "$installed_node" ] || return 1
    case "$process_node" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    process_digest="$(file_digest "$process_root/exe")" || return 1
    installed_digest="$(file_digest "$DAEMON_BINARY")" || return 1
    [ "$process_digest" = "$installed_digest" ] || return 1
    PROCESS_IMAGE=$process_node:$process_digest
    stat_line="$($TOYBOX_BIN cat "$process_root/stat" 2>/dev/null)" || return 1
    after=${stat_line##*) }
    [ "$after" != "$stat_line" ] || return 1
    set -- $after
    [ "$#" -ge 20 ] || return 1
    PROCESS_STATE=$1
    shift 19
    PROCESS_START=$1
    case "$PROCESS_STATE" in [A-Z]) ;; *) return 1 ;; esac
    case "$PROCESS_START" in *[!0-9]*|'') return 1 ;; esac
}

single_daemon() {
    pids="$($TOYBOX_BIN pidof "$DAEMON_NAME" 2>/dev/null)" || return 1
    set -- $pids
    [ "$#" -eq 1 ] || return 1
    DAEMON_PID=$1
    read_process "$DAEMON_PID"
    DAEMON_START=$PROCESS_START
    DAEMON_STATE=$PROCESS_STATE
    DAEMON_IMAGE=$PROCESS_IMAGE
}

exact_daemon() {
    expected_pid=$1
    expected_start=$2
    single_daemon || return 1
    [ "$DAEMON_PID" = "$expected_pid" ] && [ "$DAEMON_START" = "$expected_start" ] &&
        [ "$DAEMON_IMAGE" = "$3" ]
}

wait_stopped() {
    expected_pid=$1
    expected_start=$2
    attempts=0
    while [ "$attempts" -lt 20 ]; do
        exact_daemon "$expected_pid" "$expected_start" "$3" || return 1
        [ "$DAEMON_STATE" = T ] && return 0
        attempts=$((attempts + 1))
        "$TOYBOX_BIN" sleep 0.1
    done
    return 1
}

wait_running() {
    expected_pid=$1
    expected_start=$2
    attempts=0
    while [ "$attempts" -lt 20 ]; do
        exact_daemon "$expected_pid" "$expected_start" "$3" || return 1
        case "$DAEMON_STATE" in T|Z|X) ;; *) return 0 ;; esac
        attempts=$((attempts + 1))
        "$TOYBOX_BIN" sleep 0.1
    done
    return 1
}

inspect() {
    single_daemon || fail 'exactly one trusted root daemon is required'
    case "$DAEMON_STATE" in Z|X) fail 'the installed daemon is not live' ;; esac
    printf '%s|%s|%s\n' "$DAEMON_PID" "$DAEMON_START" "$DAEMON_IMAGE"
}

stop_daemon() {
    [ "$#" -eq 3 ] || fail 'a pinned PID, start time, and image are required'
    exact_daemon "$1" "$2" "$3" || fail 'daemon identity changed before freeze'
    [ "$DAEMON_STATE" != T ] || fail 'daemon was already stopped outside this upgrade'
    "$TOYBOX_BIN" kill -STOP "$1" 2>/dev/null || fail 'SIGSTOP failed'
    wait_stopped "$1" "$2" "$3" || fail 'the stopped daemon identity could not be proved'
}

verify_stopped() {
    [ "$#" -eq 3 ] || fail 'a pinned PID, start time, and image are required'
    exact_daemon "$1" "$2" "$3" || fail 'the frozen daemon identity changed'
    [ "$DAEMON_STATE" = T ] || fail 'the pinned daemon is not stopped'
}

resume_daemon() {
    [ "$#" -eq 3 ] || fail 'a pinned PID, start time, and image are required'
    exact_daemon "$1" "$2" "$3" || fail 'daemon identity changed before resume'
    if [ "$DAEMON_STATE" = T ]; then
        "$TOYBOX_BIN" kill -CONT "$1" 2>/dev/null || fail 'SIGCONT failed'
    fi
    wait_running "$1" "$2" "$3" || fail 'the resumed daemon identity could not be proved'
}

[ "$($TOYBOX_BIN id -u 2>/dev/null)" = 0 ] || fail 'root is required'
safe_toybox || fail 'trusted toybox is unavailable'
if [ "${1:-}" != --pid1 ]; then
    exec "$TOYBOX_BIN" nsenter -t 1 -m -- "$SYSTEM_SHELL" "$0" --pid1 "$@"
fi
shift
pid1_mount_namespace || fail 'daemon is not visible in the PID1 mount namespace'
safe_daemon_binary || fail 'the installed daemon binary is unavailable or unsafe'
command=${1:-}
[ "$#" -gt 0 ] && shift
case "$command" in
    --inspect) [ "$#" -eq 0 ] || fail 'unexpected arguments'; inspect ;;
    --stop) stop_daemon "$@" ;;
    --verify) verify_stopped "$@" ;;
    --resume) resume_daemon "$@" ;;
    *) fail 'usage: upgrade-freeze.sh --inspect|--stop PID START IMAGE|--verify PID START IMAGE|--resume PID START IMAGE' ;;
esac
