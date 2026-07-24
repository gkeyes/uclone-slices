#!/system/bin/sh

set -u

MODULE_ID=uclone-slices-v2
ACTIVE_MODULE=/data/adb/modules/$MODULE_ID
STAGED_MODULE=/data/adb/modules_update/$MODULE_ID
RUNTIME_ROOT=/data/adb/uclone-slices-v2
PID_FILE=$RUNTIME_ROOT/ucloned.pid
SLOTCTL=$ACTIVE_MODULE/bin/slotctl
TOYBOX=/system/bin/toybox
PROBE_PACKAGE=com.uclone.slices.v2.protocol_probe_absent
PROBE_AGGREGATE=$RUNTIME_ROOT/packages/$PROBE_PACKAGE/aggregate.json
PROBE_CE=/data/misc_ce/0/uclone-slices-v2/slots/$PROBE_PACKAGE
PROBE_DE=/data/misc_de/0/uclone-slices-v2/slots/$PROBE_PACKAGE

file_hash() {
    path=$1
    if [ ! -e "$path" ]; then
        printf '%s' missing
        return
    fi
    output=$("$TOYBOX" sha256sum "$path" 2>&1)
    status=$?
    if [ "$status" -ne 0 ]; then
        printf 'error:%s' "$output"
        return
    fi
    printf '%s' "${output%% *}"
}

printf '%s\n' '=== UClone Slices V2 reset protocol probe ==='
printf '%s\n' 'script_version=1'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'date=%s\n' "$(date 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    exit 2
fi

printf '%s\n' '--- module and process ---'
printf 'active_version=%s\n' \
    "$(sed -n 's/^version=//p' "$ACTIVE_MODULE/module.prop" 2>/dev/null)"
printf 'staged_version=%s\n' \
    "$(sed -n 's/^version=//p' "$STAGED_MODULE/module.prop" 2>/dev/null)"
ACTIVE_RUNTIME_HASH=$(file_hash "$ACTIVE_MODULE/bin/ucloned")
STAGED_RUNTIME_HASH=$(file_hash "$STAGED_MODULE/bin/ucloned")
printf 'active_runtime_sha256=%s\n' "$ACTIVE_RUNTIME_HASH"
printf 'staged_runtime_sha256=%s\n' "$STAGED_RUNTIME_HASH"
PID=
RUNNING_EXE=missing
RUNNING_RUNTIME_HASH=missing
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE" 2>/dev/null)
    case "$PID" in
        ''|*[!0-9]*) ;;
        *)
            if kill -0 "$PID" 2>/dev/null; then
                RUNNING_EXE=$(readlink "/proc/$PID/exe" 2>/dev/null)
                RUNNING_RUNTIME_HASH=$(file_hash "/proc/$PID/exe")
            fi
            ;;
    esac
fi
printf 'runtime_pid=%s\n' "$PID"
printf 'running_exe=%s\n' "$RUNNING_EXE"
printf 'running_runtime_sha256=%s\n' "$RUNNING_RUNTIME_HASH"

printf '%s\n' '--- transport ---'
PROBE_OUTPUT=$(
    printf '{"op":"probe"}\n' |
        "$TOYBOX" timeout -s 9 5 "$SLOTCTL" rpc 2>&1
)
PROBE_STATUS=$?
printf 'probe_exit=%s\n' "$PROBE_STATUS"
printf 'probe_output=%s\n' "$PROBE_OUTPUT"

printf '%s\n' '--- safe capability target ---'
PACKAGE_PATH=$(pm path "$PROBE_PACKAGE" 2>&1)
PACKAGE_PATH_STATUS=$?
printf 'probe_package_path_exit=%s\n' "$PACKAGE_PATH_STATUS"
printf 'probe_package_path_output=%s\n' "$PACKAGE_PATH"
printf 'probe_aggregate=%s\n' "$([ -e "$PROBE_AGGREGATE" ] && printf present || printf missing)"
printf 'probe_ce_slots=%s\n' "$([ -e "$PROBE_CE" ] && printf present || printf missing)"
printf 'probe_de_slots=%s\n' "$([ -e "$PROBE_DE" ] && printf present || printf missing)"

printf '%s\n' '--- reset field capability ---'
RESET_STATUS=not_run
RESET_OUTPUT=not_run
SAFE_TARGET=yes
case "$PACKAGE_PATH" in
    *package:*) SAFE_TARGET=no ;;
esac
if [ -e "$PROBE_AGGREGATE" ] || [ -e "$PROBE_CE" ] || [ -e "$PROBE_DE" ]; then
    SAFE_TARGET=no
fi
if [ "$SAFE_TARGET" = yes ]; then
    RESET_OUTPUT=$(
        printf '{"op":"enroll","package":"%s","reset":true}\n' "$PROBE_PACKAGE" |
            "$TOYBOX" timeout -s 9 5 "$SLOTCTL" rpc 2>&1
    )
    RESET_STATUS=$?
fi
printf 'safe_target=%s\n' "$SAFE_TARGET"
printf 'reset_probe_exit=%s\n' "$RESET_STATUS"
printf 'reset_probe_output=%s\n' "$RESET_OUTPUT"

printf '%s\n' '--- classification ---'
if [ "$SAFE_TARGET" != yes ]; then
    RESULT=probe_target_not_empty
elif [ "$PROBE_STATUS" -ne 0 ] || [ "$RESET_STATUS" -ne 0 ]; then
    RESULT=runtime_transport_failed
elif [ "$ACTIVE_RUNTIME_HASH" != "$RUNNING_RUNTIME_HASH" ]; then
    RESULT=running_binary_differs_from_active_module
else
    case "$RESET_OUTPUT" in
        *'"operation_failed"'*) RESULT=runtime_accepts_reset_protocol ;;
        *'"invalid_request"'*) RESULT=runtime_rejects_reset_protocol ;;
        *) RESULT=unexpected_reset_probe_response ;;
    esac
fi
printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' '=== end ==='
