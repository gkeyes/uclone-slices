#!/system/bin/sh

set -u

MODULE_ID=uclone-slices-v2
ACTIVE_MODULE=/data/adb/modules/$MODULE_ID
STAGED_MODULE=/data/adb/modules_update/$MODULE_ID
RUNTIME_ROOT=/data/adb/uclone-slices-v2
PID_FILE=$RUNTIME_ROOT/ucloned.pid
SOCKET=$RUNTIME_ROOT/runtime.sock
RUNTIME_BIN=$ACTIVE_MODULE/bin/ucloned
SLOTCTL_BIN=$ACTIVE_MODULE/bin/slotctl
SERVICE_SCRIPT=$ACTIVE_MODULE/service.sh

printf '%s\n' '=== Force-start UClone Slices V2 Runtime ==='

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    printf '%s\n' 'Run this script from a root shell.'
    exit 2
fi

if [ ! -d "$ACTIVE_MODULE" ]; then
    if [ -d "$STAGED_MODULE" ]; then
        printf '%s\n' 'RESULT=module_staged_not_active'
        printf '%s\n' 'The module update is staged but is not the active module yet.'
    else
        printf '%s\n' 'RESULT=active_module_missing'
    fi
    exit 1
fi

if [ -d "$STAGED_MODULE" ]; then
    ACTIVE_VERSION=$(sed -n 's/^version=//p' "$ACTIVE_MODULE/module.prop")
    STAGED_VERSION=$(sed -n 's/^version=//p' "$STAGED_MODULE/module.prop")
    printf 'active_version=%s\n' "$ACTIVE_VERSION"
    printf 'staged_version=%s\n' "$STAGED_VERSION"
    if [ -n "$STAGED_VERSION" ] && [ "$ACTIVE_VERSION" != "$STAGED_VERSION" ]; then
        printf '%s\n' 'RESULT=module_update_staged_not_active'
        printf '%s\n' 'Reboot to activate the staged module before restarting Runtime.'
        exit 1
    fi
fi

for required in "$RUNTIME_BIN" "$SLOTCTL_BIN" "$SERVICE_SCRIPT"; do
    if [ ! -f "$required" ]; then
        printf 'missing=%s\n' "$required"
        printf '%s\n' 'RESULT=module_incomplete'
        exit 1
    fi
done

chmod 0755 "$RUNTIME_BIN" "$SLOTCTL_BIN" "$SERVICE_SCRIPT" || {
    printf '%s\n' 'RESULT=permission_update_failed'
    exit 1
}

for process_root in /proc/[0-9]*; do
    [ -d "$process_root" ] || continue
    process_id=${process_root##*/}
    executable=$(readlink "$process_root/exe" 2>/dev/null)
    case "$executable" in
        "$RUNTIME_BIN"|"$RUNTIME_BIN (deleted)")
            printf 'stopping_pid=%s\n' "$process_id"
            kill -9 "$process_id" 2>/dev/null || {
                printf 'RESULT=failed_to_stop_pid_%s\n' "$process_id"
                exit 1
            }
            ;;
    esac
done

rm -f "$PID_FILE" "$SOCKET" || {
    printf '%s\n' 'RESULT=stale_control_cleanup_failed'
    exit 1
}

/system/bin/sh "$SERVICE_SCRIPT" || {
    printf '%s\n' 'RESULT=service_start_failed'
    exit 1
}

if [ ! -f "$PID_FILE" ]; then
    printf '%s\n' 'RESULT=pid_file_missing_after_start'
    exit 1
fi

NEW_PID=$(cat "$PID_FILE" 2>/dev/null)
case "$NEW_PID" in
    ''|*[!0-9]*)
        printf 'pid=%s\n' "$NEW_PID"
        printf '%s\n' 'RESULT=invalid_pid_after_start'
        exit 1
        ;;
esac

if ! kill -0 "$NEW_PID" 2>/dev/null; then
    printf 'pid=%s\n' "$NEW_PID"
    printf '%s\n' 'RESULT=runtime_exited_during_start'
    if [ -f "$RUNTIME_ROOT/ucloned.log" ]; then
        tail -n 40 "$RUNTIME_ROOT/ucloned.log"
    fi
    exit 1
fi

printf 'pid=%s\n' "$NEW_PID"
printf 'pid_exe=%s\n' "$(readlink "/proc/$NEW_PID/exe" 2>/dev/null)"
if [ -S "$SOCKET" ]; then
    printf '%s\n' 'RESULT=runtime_started_socket_ready'
else
    printf '%s\n' 'RESULT=runtime_started_socket_pending'
fi
printf '%s\n' 'Return to Manager and tap Refresh.'
