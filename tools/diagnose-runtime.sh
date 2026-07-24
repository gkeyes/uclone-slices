#!/system/bin/sh

set -u

MODULE_ID=uclone-slices-v2
ACTIVE_MODULE=/data/adb/modules/$MODULE_ID
STAGED_MODULE=/data/adb/modules_update/$MODULE_ID
RUNTIME_ROOT=/data/adb/uclone-slices-v2
PID_FILE=$RUNTIME_ROOT/ucloned.pid
SOCKET=$RUNTIME_ROOT/runtime.sock
LOG_FILE=$RUNTIME_ROOT/ucloned.log
RUNTIME_BIN=$ACTIVE_MODULE/bin/ucloned
SLOTCTL_BIN=$ACTIVE_MODULE/bin/slotctl
TOYBOX_BIN=/system/bin/toybox

print_path() {
    label=$1
    path=$2
    if [ -e "$path" ] || [ -L "$path" ]; then
        printf '%s=present\n' "$label"
        ls -ld "$path" 2>&1
    else
        printf '%s=missing\n' "$label"
    fi
}

printf '%s\n' '=== UClone Slices V2 Runtime diagnosis ==='
printf '%s\n' 'script_version=3'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'date=%s\n' "$(date 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    printf '%s\n' 'Run this script from a root shell.'
    exit 2
fi

printf '%s\n' '--- module ---'
print_path active_module "$ACTIVE_MODULE"
print_path staged_module "$STAGED_MODULE"
for marker in disable remove update; do
    print_path "active_$marker" "$ACTIVE_MODULE/$marker"
done
if [ -f "$ACTIVE_MODULE/module.prop" ]; then
    sed -n '/^id=/p;/^version=/p;/^versionCode=/p' "$ACTIVE_MODULE/module.prop"
fi
printf 'active_version=%s\n' \
    "$(sed -n 's/^version=//p' "$ACTIVE_MODULE/module.prop" 2>/dev/null)"
printf 'staged_version=%s\n' \
    "$(sed -n 's/^version=//p' "$STAGED_MODULE/module.prop" 2>/dev/null)"
print_path runtime_binary "$RUNTIME_BIN"
print_path slotctl_binary "$SLOTCTL_BIN"

printf '%s\n' '--- daemon ---'
print_path runtime_root "$RUNTIME_ROOT"
print_path pid_file "$PID_FILE"
PID=
PID_STATE=missing
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE" 2>/dev/null)
    case "$PID" in
        ''|*[!0-9]*) PID_STATE=invalid ;;
        *)
            if kill -0 "$PID" 2>/dev/null; then
                PID_STATE=live
                printf 'pid=%s\n' "$PID"
                printf 'pid_exe=%s\n' "$(readlink "/proc/$PID/exe" 2>/dev/null)"
                printf 'pid_mount_ns=%s\n' "$(readlink "/proc/$PID/ns/mnt" 2>/dev/null)"
            else
                PID_STATE=stale
            fi
            ;;
    esac
fi
printf 'pid_state=%s\n' "$PID_STATE"
printf 'pidof_ucloned=%s\n' "$(pidof ucloned 2>/dev/null)"
printf 'init_mount_ns=%s\n' "$(readlink /proc/1/ns/mnt 2>/dev/null)"
printf 'user0_state=%s\n' \
    "$(cmd activity get-started-user-state 0 2>&1)"
printf 'boot_completed=%s\n' "$(getprop sys.boot_completed 2>/dev/null)"

printf '%s\n' '--- socket and probe ---'
print_path runtime_socket "$SOCKET"
if [ -S "$SOCKET" ]; then
    SOCKET_STATE=socket
elif [ -e "$SOCKET" ]; then
    SOCKET_STATE=wrong_type
else
    SOCKET_STATE=missing
fi
printf 'socket_state=%s\n' "$SOCKET_STATE"

RPC_STATUS=127
RPC_OUTPUT=
if [ -x "$SLOTCTL_BIN" ]; then
    RPC_OUTPUT=$(
        printf '{"op":"probe"}\n' |
            "$TOYBOX_BIN" timeout -s 9 5 "$SLOTCTL_BIN" rpc 2>&1
    )
    RPC_STATUS=$?
fi
printf 'rpc_exit=%s\n' "$RPC_STATUS"
printf 'rpc_output=%s\n' "$RPC_OUTPUT"

printf '%s\n' '--- latest Runtime log excerpt ---'
if [ -f "$LOG_FILE" ]; then
    tail -n 40 "$LOG_FILE" 2>&1
else
    printf '%s\n' 'log=missing'
fi

printf '%s\n' '--- classification ---'
if [ ! -d "$ACTIVE_MODULE" ] && [ -d "$STAGED_MODULE" ]; then
    RESULT=module_staged_not_active
elif [ ! -d "$ACTIVE_MODULE" ]; then
    RESULT=active_module_missing
elif [ -e "$ACTIVE_MODULE/disable" ]; then
    RESULT=module_disabled
elif [ -e "$ACTIVE_MODULE/remove" ]; then
    RESULT=module_marked_for_removal
elif [ ! -x "$RUNTIME_BIN" ] || [ ! -x "$SLOTCTL_BIN" ]; then
    RESULT=module_binary_missing_or_not_executable
elif [ "$RPC_STATUS" -eq 0 ]; then
    case "$RPC_OUTPUT" in
        *'"ok"'*'"build_id"'*) RESULT=runtime_connected ;;
        *) RESULT=runtime_replied_but_probe_failed ;;
    esac
elif [ "$PID_STATE" != live ]; then
    RESULT=daemon_not_running
elif [ "$SOCKET_STATE" != socket ]; then
    RESULT=daemon_live_but_socket_unavailable
else
    RESULT=rpc_transport_failed
fi

printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' '=== end ==='
