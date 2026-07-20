#!/system/bin/sh

set -u
umask 077

case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 0 ;;
esac
PROFILE_FILE=$SCRIPT_DIR/target-profile.sh
[ -f "$PROFILE_FILE" ] && [ ! -L "$PROFILE_FILE" ] || exit 0
. "$PROFILE_FILE"
RUNTIME_ROOT=$UCLONE_RUNTIME_ROOT
RUN_ROOT=$RUNTIME_ROOT/run
READY_FILE=$RUN_ROOT/startup-gate.ready
LOG_FILE=$RUNTIME_ROOT/logs/ucloned.log
TOYBOX_BIN=/system/bin/toybox
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
RUNTIME_BIN=$SCRIPT_DIR/bin/ucloned
BOOT_RECONCILER=$SCRIPT_DIR/boot-completed.sh
EMERGENCY_CONTAINMENT=$SCRIPT_DIR/emergency-containment.sh

launch_emergency_containment() {
    if safe_binary "$TOYBOX_BIN" && safe_binary "$EMERGENCY_CONTAINMENT"; then
        ("$EMERGENCY_CONTAINMENT" --watch >/dev/null 2>&1) &
    fi
}

abort_to_emergency() {
    launch_emergency_containment
    exit 0
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

safe_directory() {
    path="$1"
    [ -d "$path" ] && [ ! -L "$path" ] || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$path" 2>/dev/null)" = "0:0:700" ]
}

safe_log() {
    [ ! -L "$LOG_FILE" ] || return 1
    [ ! -e "$LOG_FILE" ] || [ -f "$LOG_FILE" ]
}

write_log() {
    safe_log || return 1
    printf '%s\n' "$1" >>"$LOG_FILE" 2>/dev/null
}

ready_for_boot() {
    [ -f "$READY_FILE" ] && [ ! -L "$READY_FILE" ] || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$READY_FILE" 2>/dev/null)" = "0:0:600" ] || return 1
    value="$("$TOYBOX_BIN" cat "$READY_FILE" 2>/dev/null)" || return 1
    [ "$value" = "$boot_id held" ] || [ "$value" = "$boot_id not-managed" ]
}

reassert_exact_gate() {
    if ! safe_binary "$RUNTIME_BIN"; then
        "$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1
        return 1
    fi
    "$TOYBOX_BIN" timeout -s 9 30 "$TOYBOX_BIN" nsenter -t 1 -m -- \
        "$RUNTIME_BIN" --startup-gate >>"$LOG_FILE" 2>&1
}

safe_binary "$TOYBOX_BIN" || abort_to_emergency
safe_binary "$EMERGENCY_CONTAINMENT" || abort_to_emergency
safe_binary "$RUNTIME_BIN" || abort_to_emergency
safe_binary "$BOOT_RECONCILER" || abort_to_emergency
safe_directory "$RUNTIME_ROOT" || abort_to_emergency
safe_directory "$RUN_ROOT" || abort_to_emergency
safe_directory "$RUNTIME_ROOT/logs" || abort_to_emergency
safe_log || abort_to_emergency
boot_id="$("$TOYBOX_BIN" cat "$BOOT_ID_FILE" 2>/dev/null)" || abort_to_emergency
case "$boot_id" in *[!A-Za-z0-9-]*|'') abort_to_emergency ;; esac

(
    while ! ready_for_boot; do
        [ ! -e "$RUNTIME_ROOT/disabled" ] || exit 0
        "$TOYBOX_BIN" sleep 1
    done
    "$BOOT_RECONCILER" >/dev/null 2>&1
    while [ ! -e "$RUNTIME_ROOT/disabled" ]; do
        "$TOYBOX_BIN" nsenter -t 1 -m -- "$RUNTIME_BIN" >>"$LOG_FILE" 2>&1
        status=$?
        write_log "ucloned exited with status $status; exact gate revalidation required"
        until reassert_exact_gate; do
            write_log 'exact startup containment remains unproved; daemon restart withheld'
            "$TOYBOX_BIN" sleep 5
        done
        "$TOYBOX_BIN" sleep 2
    done
) &

exit 0
