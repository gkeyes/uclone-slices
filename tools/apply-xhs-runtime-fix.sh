#!/system/bin/sh

set -u

MODULE_ID=uclone-slices-v2
ACTIVE_MODULE=/data/adb/modules/$MODULE_ID
STAGED_MODULE=/data/adb/modules_update/$MODULE_ID
RUNTIME_ROOT=/data/adb/uclone-slices-v2
PID_FILE=$RUNTIME_ROOT/ucloned.pid
SOCKET=$RUNTIME_ROOT/runtime.sock
ACTIVE_RUNTIME=$ACTIVE_MODULE/bin/ucloned
SLOTCTL=$ACTIVE_MODULE/bin/slotctl
SERVICE=$ACTIVE_MODULE/service.sh
case "$0" in
    */*) SCRIPT_DIR=${0%/*} ;;
    *) SCRIPT_DIR=. ;;
esac
SOURCE_RUNTIME=$SCRIPT_DIR/bin/ucloned
EXPECTED_RUNTIME_SIZE=957632
TOYBOX=/system/bin/toybox

file_size() {
    file=$1
    if [ -f "$file" ]; then
        "$TOYBOX" stat -c '%s' "$file" 2>/dev/null
    else
        printf '%s\n' missing
    fi
}

restore_previous_runtime() {
    for restore_process_root in /proc/[0-9]*; do
        [ -d "$restore_process_root" ] || continue
        restore_process_id=${restore_process_root##*/}
        restore_executable=$(readlink "$restore_process_root/exe" 2>/dev/null)
        case "$restore_executable" in
            "$ACTIVE_RUNTIME"|"$ACTIVE_RUNTIME (deleted)")
                kill -9 "$restore_process_id" 2>/dev/null
                ;;
        esac
    done
    failed_runtime=$ACTIVE_MODULE/bin/.ucloned.failed.$$
    if [ -f "$ACTIVE_RUNTIME" ]; then
        mv -f "$ACTIVE_RUNTIME" "$failed_runtime" 2>/dev/null
    fi
    mv -f "$BACKUP_RUNTIME" "$ACTIVE_RUNTIME" 2>/dev/null
    chmod 0755 "$ACTIVE_RUNTIME" 2>/dev/null
    rm -f "$PID_FILE" "$SOCKET"
    /system/bin/sh "$SERVICE" >/dev/null 2>&1
    rm -f "$failed_runtime"
}

printf '%s\n' '=== Apply UClone Slices V2 XHS Runtime fix ==='
printf '%s\n' 'script_version=2'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    exit 2
fi

for required in "$SOURCE_RUNTIME" "$ACTIVE_RUNTIME" "$SLOTCTL" "$SERVICE"; do
    if [ ! -f "$required" ]; then
        printf 'missing=%s\n' "$required"
        printf '%s\n' 'RESULT=required_file_missing'
        exit 1
    fi
done

SOURCE_SIZE=$(file_size "$SOURCE_RUNTIME")
ACTIVE_SIZE=$(file_size "$ACTIVE_RUNTIME")
STAGED_SIZE=$(file_size "$STAGED_MODULE/bin/ucloned")
printf 'source_runtime_size=%s\n' "$SOURCE_SIZE"
printf 'active_runtime_size_before=%s\n' "$ACTIVE_SIZE"
printf 'staged_runtime_size=%s\n' "$STAGED_SIZE"

if [ "$SOURCE_SIZE" != "$EXPECTED_RUNTIME_SIZE" ]; then
    printf 'expected_runtime_size=%s\n' "$EXPECTED_RUNTIME_SIZE"
    printf '%s\n' 'RESULT=bundle_runtime_mismatch'
    exit 1
fi

if [ -f "$PID_FILE" ]; then
    OLD_PID=$("$TOYBOX" cat "$PID_FILE" 2>/dev/null)
    printf 'old_pid=%s\n' "$OLD_PID"
    printf 'old_pid_exe=%s\n' "$(readlink "/proc/$OLD_PID/exe" 2>/dev/null)"
    printf 'old_running_size=%s\n' "$(file_size "/proc/$OLD_PID/exe")"
fi

for process_root in /proc/[0-9]*; do
    [ -d "$process_root" ] || continue
    process_id=${process_root##*/}
    executable=$(readlink "$process_root/exe" 2>/dev/null)
    case "$executable" in
        "$ACTIVE_RUNTIME"|"$ACTIVE_RUNTIME (deleted)")
            printf 'stopping_pid=%s\n' "$process_id"
            kill -9 "$process_id" 2>/dev/null || {
                printf '%s\n' 'RESULT=failed_to_stop_runtime'
                exit 1
            }
            ;;
    esac
done

TEMP_RUNTIME=$ACTIVE_MODULE/bin/.ucloned.new.$$
BACKUP_RUNTIME=$ACTIVE_MODULE/bin/.ucloned.previous.$$
rm -f "$TEMP_RUNTIME" "$BACKUP_RUNTIME"
cp "$SOURCE_RUNTIME" "$TEMP_RUNTIME" || {
    printf '%s\n' 'RESULT=copy_new_runtime_failed'
    exit 1
}
chmod 0755 "$TEMP_RUNTIME" || {
    rm -f "$TEMP_RUNTIME"
    printf '%s\n' 'RESULT=chmod_new_runtime_failed'
    exit 1
}
mv "$ACTIVE_RUNTIME" "$BACKUP_RUNTIME" || {
    rm -f "$TEMP_RUNTIME"
    printf '%s\n' 'RESULT=backup_active_runtime_failed'
    exit 1
}
if ! mv "$TEMP_RUNTIME" "$ACTIVE_RUNTIME"; then
    mv "$BACKUP_RUNTIME" "$ACTIVE_RUNTIME" 2>/dev/null
    printf '%s\n' 'RESULT=activate_new_runtime_failed'
    exit 1
fi

ACTIVE_SIZE=$(file_size "$ACTIVE_RUNTIME")
printf 'active_runtime_size_after=%s\n' "$ACTIVE_SIZE"
if [ "$ACTIVE_SIZE" != "$EXPECTED_RUNTIME_SIZE" ]; then
    restore_previous_runtime
    printf '%s\n' 'RESULT=active_runtime_verification_failed'
    exit 1
fi

rm -f "$PID_FILE" "$SOCKET"
if ! /system/bin/sh "$SERVICE"; then
    restore_previous_runtime
    printf '%s\n' 'RESULT=new_runtime_start_failed_previous_restored'
    exit 1
fi

NEW_PID=$("$TOYBOX" cat "$PID_FILE" 2>/dev/null)
case "$NEW_PID" in
    ''|*[!0-9]*)
        restore_previous_runtime
        printf 'new_pid=%s\n' "$NEW_PID"
        printf '%s\n' 'RESULT=new_runtime_pid_invalid_previous_restored'
        exit 1
        ;;
esac

while [ ! -S "$SOCKET" ]; do
    if ! kill -0 "$NEW_PID" 2>/dev/null; then
        printf '%s\n' '--- Runtime log ---'
        tail -n 40 "$RUNTIME_ROOT/ucloned.log" 2>&1
        restore_previous_runtime
        printf '%s\n' 'RESULT=new_runtime_exited_previous_restored'
        exit 1
    fi
    sleep 1
done

printf 'new_pid=%s\n' "$NEW_PID"
printf 'new_pid_exe=%s\n' "$(readlink "/proc/$NEW_PID/exe" 2>/dev/null)"
printf 'new_running_size=%s\n' "$(file_size "/proc/$NEW_PID/exe")"

RPC_OUTPUT=$(
    printf '{"op":"get_package","package":"com.xingin.xhs"}\n' |
        "$SLOTCTL" rpc 2>&1
)
RPC_STATUS=$?
printf 'rpc_exit=%s\n' "$RPC_STATUS"
printf 'rpc_output=%s\n' "$RPC_OUTPUT"

rm -f "$BACKUP_RUNTIME"

case "$RPC_OUTPUT" in
    *'"ok":'*'"active_slot":"slot-1"'*)
        printf '%s\n' 'RESULT=xhs_slot_recovered'
        ;;
    *)
        printf '%s\n' '--- latest Runtime log ---'
        tail -n 40 "$RUNTIME_ROOT/ucloned.log" 2>&1
        printf '%s\n' 'RESULT=new_runtime_active_but_xhs_recovery_failed'
        exit 1
        ;;
esac

printf '%s\n' 'Return to Manager and tap Refresh.'
