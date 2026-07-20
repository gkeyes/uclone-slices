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
REQUEST_FILE=$RUN_ROOT/reconcile.requested
LAST_RESPONSE=$RUN_ROOT/reconcile.last
LOG_FILE=$RUNTIME_ROOT/logs/reconcile.log
TOYBOX_BIN=/system/bin/toybox
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
SLOTCTL_BIN=$SCRIPT_DIR/bin/slotctl
STATE_HELPER=$SCRIPT_DIR/boot-state.sh

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

safe_control_file() {
    path="$1"
    [ ! -L "$path" ] || return 1
    [ ! -e "$path" ] || [ -f "$path" ]
}

write_log() {
    safe_control_file "$LOG_FILE" || return 1
    printf '%s\n' "$1" >>"$LOG_FILE" 2>/dev/null
}

ready_for_boot() {
    [ -f "$READY_FILE" ] && [ ! -L "$READY_FILE" ] || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$READY_FILE" 2>/dev/null)" = "0:0:600" ] || return 1
    value="$("$TOYBOX_BIN" cat "$READY_FILE" 2>/dev/null)" || return 1
    [ "$value" = "$boot_id held" ] || [ "$value" = "$boot_id not-managed" ]
}

prepare_request() {
    temporary=$RUN_ROOT/.reconcile.requested.$$
    safe_control_file "$REQUEST_FILE" || return 1
    [ ! -e "$temporary" ] || return 1
    printf '%s\n' "$boot_id" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$REQUEST_FILE" 2>/dev/null
}

publish_response() {
    frame="$1"
    temporary=$RUN_ROOT/.reconcile.last.$$
    safe_control_file "$LAST_RESPONSE" || return 1
    [ ! -e "$temporary" ] || return 1
    printf '%s\n' "$frame" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$LAST_RESPONSE" 2>/dev/null
}

discard_request() {
    safe_control_file "$REQUEST_FILE" || return 1
    "$TOYBOX_BIN" rm -f "$REQUEST_FILE" 2>/dev/null
}

run_reconcile() {
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        "$TOYBOX_BIN" timeout -s 9 40 "$TOYBOX_BIN" nsenter -t 1 -m -- \
            "$SLOTCTL_BIN" reconcile
    else
        "$TOYBOX_BIN" timeout -s 9 40 "$TOYBOX_BIN" nsenter -t 1 -m -- \
            "$SLOTCTL_BIN" reconcile "$UCLONE_TARGET_PACKAGE"
    fi
}

safe_binary "$TOYBOX_BIN" || exit 0
safe_binary "$SLOTCTL_BIN" || exit 0
safe_binary "$STATE_HELPER" || exit 0
safe_directory "$RUNTIME_ROOT" || exit 0
safe_directory "$RUN_ROOT" || exit 0
safe_directory "$RUNTIME_ROOT/logs" || exit 0
safe_control_file "$LOG_FILE" || exit 0
boot_id="$("$TOYBOX_BIN" cat "$BOOT_ID_FILE" 2>/dev/null)" || exit 0
case "$boot_id" in *[!A-Za-z0-9-]*|'') exit 0 ;; esac
prepare_request || exit 0

# shellcheck disable=SC1090
. "$STATE_HELPER"

(
    delay=2
    while [ -f "$REQUEST_FILE" ] && [ ! -L "$REQUEST_FILE" ]; do
        if ! ready_for_boot; then
            "$TOYBOX_BIN" sleep "$delay"
            [ "$delay" -ge 30 ] || delay=$((delay + 2))
            continue
        fi
        frame="$(run_reconcile 2>>"$LOG_FILE")"
        status=$?
        if [ "$status" -eq 0 ]; then
            classification="$(classify_reconcile_frame "$frame")"
            publish_response "$frame" || classification=invalid
            marker_action="$(reconcile_marker_action "$classification")"
            case "$marker_action:$classification" in
                retire:terminal)
                    discard_request || write_log 'terminal reconcile proved but marker retirement failed'
                    exit 0
                    ;;
                retain:locked)
                    write_log 'user0 remains locked; durable reconcile request retained'
                    delay=2
                    ;;
                *)
                    write_log 'reconcile returned an invalid typed response; request retained'
                    ;;
            esac
        else
            write_log "reconcile attempt failed with status $status; request retained"
        fi
        "$TOYBOX_BIN" sleep "$delay"
        [ "$delay" -ge 30 ] || delay=$((delay + 2))
    done
) &

exit 0
