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
RUN_ROOT=$RUNTIME_ROOT/run
READY_FILE=$RUN_ROOT/startup-gate.ready
PENDING_FILE=$RUN_ROOT/startup-gate.pending
LOG_FILE=$RUNTIME_ROOT/logs/startup-gate.log
TOYBOX_BIN=/system/bin/toybox
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
RUNTIME_BIN=$SCRIPT_DIR/bin/ucloned
EMERGENCY_CONTAINMENT=$SCRIPT_DIR/emergency-containment.sh
MODE=${1:-}
case "$MODE" in ''|--once) ;; *) exit 2 ;; esac

launch_emergency_containment() {
    status=1
    if safe_binary "$TOYBOX_BIN" && safe_binary "$EMERGENCY_CONTAINMENT"; then
        "$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1 && status=0
        ("$EMERGENCY_CONTAINMENT" --watch >/dev/null 2>&1) &
    fi
    return "$status"
}

abort_to_emergency() {
    status=0
    launch_emergency_containment || status=1
    exit "$status"
}

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

safe_control_file() {
    path="$1"
    [ ! -L "$path" ] || return 1
    [ ! -e "$path" ] || [ -f "$path" ]
}

write_log() {
    safe_control_file "$LOG_FILE" || return 1
    printf '%s\n' "$1" >>"$LOG_FILE" 2>/dev/null
}

publish_ready() {
    value="$1"
    temporary=$RUN_ROOT/.startup-gate.ready.new
    safe_control_file "$READY_FILE" || return 1
    [ ! -e "$temporary" ] || return 1
    printf '%s\n' "$boot_id $value" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$READY_FILE" 2>/dev/null || return 1
    "$TOYBOX_BIN" rm -f "$PENDING_FILE" 2>/dev/null || return 1
}

pending_for_boot() {
    [ -f "$PENDING_FILE" ] && [ ! -L "$PENDING_FILE" ] || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$PENDING_FILE" 2>/dev/null)" = "0:0:600" ] || return 1
    [ "$("$TOYBOX_BIN" cat "$PENDING_FILE" 2>/dev/null)" = "$boot_id" ]
}

attempt_gate() {
    timeout_seconds="$1"
    safe_binary "$RUNTIME_BIN" || return 1
    result="$("$TOYBOX_BIN" timeout -s 9 "$timeout_seconds" \
        "$TOYBOX_BIN" nsenter -t 1 -m -- "$RUNTIME_BIN" --startup-gate 2>>"$LOG_FILE")"
    status=$?
    [ "$status" -eq 0 ] || return 1
    case "$result" in
        held|not-managed) publish_ready "$result" ;;
        *) return 1 ;;
    esac
}

safe_binary "$TOYBOX_BIN" || abort_to_emergency
safe_binary "$EMERGENCY_CONTAINMENT" || abort_to_emergency
[ -d "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || abort_to_emergency
[ -d "$RUNTIME_ROOT/logs" ] && [ ! -L "$RUNTIME_ROOT/logs" ] || abort_to_emergency
safe_control_file "$LOG_FILE" || abort_to_emergency
boot_id="$("$TOYBOX_BIN" cat "$BOOT_ID_FILE" 2>/dev/null)" || abort_to_emergency
case "$boot_id" in *[!A-Za-z0-9-]*|'') abort_to_emergency ;; esac
pending_for_boot || abort_to_emergency

if [ "$MODE" = --once ]; then
    if attempt_gate 8; then
        write_log "startup gate proved synchronously for boot $boot_id"
        exit 0
    fi
    write_log "synchronous startup gate failed for boot $boot_id; emergency containment retained"
    abort_to_emergency
fi

delay=1
while pending_for_boot; do
    if ! safe_binary "$RUNTIME_BIN"; then
        "$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1
        write_log "startup gate runtime unavailable for boot $boot_id; containment retained"
        "$TOYBOX_BIN" sleep "$delay"
        [ "$delay" -ge 5 ] || delay=$((delay + 1))
        continue
    fi
    if attempt_gate 30; then
        write_log "startup gate proved for boot $boot_id"
        exit 0
    fi
    write_log "startup gate pending for boot $boot_id"
    "$TOYBOX_BIN" sleep "$delay"
    [ "$delay" -ge 5 ] || delay=$((delay + 1))
done

exit 0
