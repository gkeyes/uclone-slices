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
PACKAGE=$UCLONE_TARGET_PACKAGE
USER_ID=$UCLONE_TARGET_USER
RUNTIME_VERSION=1
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd
AM_BIN=/system/bin/am
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
STARTUP_GATE=$SCRIPT_DIR/startup-gate.sh
EMERGENCY_CONTAINMENT=$SCRIPT_DIR/emergency-containment.sh

management_artifact_present() {
    if [ -L "$RUNTIME_ROOT" ] || { [ -e "$RUNTIME_ROOT" ] && [ ! -d "$RUNTIME_ROOT" ]; }; then
        return 0
    fi
    [ -d "$RUNTIME_ROOT" ] || return 1
    for path in \
        "$RUNTIME_ROOT/enrollment/packages/$PACKAGE" \
        "$RUNTIME_ROOT/catalog/packages/$PACKAGE" \
        "$RUNTIME_ROOT/registry/packages/$PACKAGE" \
        "$RUNTIME_ROOT/package-state/packages/$PACKAGE" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts/$PACKAGE" \
        "$RUNTIME_ROOT/rescue-journal/packages/$PACKAGE" \
        "$RUNTIME_ROOT/state/$PACKAGE.gate" \
        "$RUNTIME_ROOT/state/.$PACKAGE.gate.retiring" \
        "$RUNTIME_ROOT/state/.$PACKAGE.gate.retired"
    do
        [ ! -e "$path" ] && [ ! -L "$path" ] || return 0
    done
    return 1
}

launch_emergency_containment() {
    if safe_toybox && safe_module_binary "$EMERGENCY_CONTAINMENT"; then
        ("$EMERGENCY_CONTAINMENT" --watch >/dev/null 2>&1) &
    elif management_artifact_present; then
        (
            "$CMD_BIN" package disable-user --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1
            "$AM_BIN" force-stop --user "$USER_ID" "$PACKAGE" >/dev/null 2>&1
        ) &
    fi
}

fail_closed() {
    launch_emergency_containment
    exit 0
}

safe_toybox() {
    [ -f "$TOYBOX_BIN" ] || return 1
    [ ! -L "$TOYBOX_BIN" ] || return 1
    [ -x "$TOYBOX_BIN" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    case "$owner" in 0:*) return 0 ;; *) return 1 ;; esac
}

ensure_dir() {
    path="$1"
    if [ -L "$path" ] || { [ -e "$path" ] && [ ! -d "$path" ]; }; then
        return 1
    fi
    "$TOYBOX_BIN" mkdir -p "$path" 2>/dev/null || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g' "$path" 2>/dev/null)" = "0:0" ] || return 1
    "$TOYBOX_BIN" chmod 700 "$path" 2>/dev/null || return 1
}

ensure_version() {
    version_file="$RUNTIME_ROOT/version"
    if [ -L "$version_file" ] || { [ -e "$version_file" ] && [ ! -f "$version_file" ]; }; then
        return 1
    fi
    if [ -f "$version_file" ]; then
        [ "$("$TOYBOX_BIN" cat "$version_file" 2>/dev/null)" = "$RUNTIME_VERSION" ] || return 1
        [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$version_file" 2>/dev/null)" = "0:0:600" ] || return 1
        return 0
    fi
    temporary="$RUNTIME_ROOT/.version.new"
    [ ! -e "$temporary" ] || return 1
    printf '%s\n' "$RUNTIME_VERSION" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$version_file" 2>/dev/null || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$version_file" 2>/dev/null)" = "0:0:600" ]
}

safe_module_binary() {
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

prepare_boot_gate() {
    run_root=$RUNTIME_ROOT/run
    ready=$run_root/startup-gate.ready
    pending=$run_root/startup-gate.pending
    temporary=$run_root/.startup-gate.pending.new
    boot_id="$("$TOYBOX_BIN" cat "$BOOT_ID_FILE" 2>/dev/null)" || return 1
    case "$boot_id" in *[!A-Za-z0-9-]*|'') return 1 ;; esac
    for stale in "$ready" "$run_root/.startup-gate.ready.new"; do
        [ ! -L "$stale" ] || return 1
        "$TOYBOX_BIN" rm -f "$stale" 2>/dev/null || return 1
    done
    [ ! -L "$pending" ] || return 1
    "$TOYBOX_BIN" rm -f "$pending" "$temporary" 2>/dev/null || return 1
    printf '%s\n' "$boot_id" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$pending" 2>/dev/null || return 1
}

safe_toybox || fail_closed
ensure_dir "$RUNTIME_ROOT" || fail_closed
ensure_version || fail_closed
ensure_dir "$RUNTIME_ROOT/logs" || fail_closed
ensure_dir "$RUNTIME_ROOT/run" || fail_closed
ensure_dir "$RUNTIME_ROOT/state" || fail_closed
ensure_dir "$RUNTIME_ROOT/journal" || fail_closed
ensure_dir "$RUNTIME_ROOT/rescue-journal" || fail_closed
ensure_dir "$RUNTIME_ROOT/registry" || fail_closed
ensure_dir "$RUNTIME_ROOT/enrollment" || fail_closed
ensure_dir "$RUNTIME_ROOT/enrollment-attempts" || fail_closed
ensure_dir "$RUNTIME_ROOT/package-state" || fail_closed
ensure_dir "$RUNTIME_ROOT/catalog" || fail_closed
safe_module_binary "$EMERGENCY_CONTAINMENT" || fail_closed
safe_module_binary "$STARTUP_GATE" || fail_closed
prepare_boot_gate || fail_closed

("$STARTUP_GATE" >/dev/null 2>&1) &

exit 0
