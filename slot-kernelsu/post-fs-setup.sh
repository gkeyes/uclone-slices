#!/system/bin/sh

set -u
umask 077

case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 1 ;;
esac
PROFILE_FILE=$SCRIPT_DIR/target-profile.sh
PROFILE_LOADER=$SCRIPT_DIR/profile-loader.sh
UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview
if [ -f "$PROFILE_LOADER" ] && [ ! -L "$PROFILE_LOADER" ]; then
    . "$PROFILE_LOADER"
    uclone_load_profile "$PROFILE_FILE" || exit 1
fi
RUNTIME_ROOT=$UCLONE_RUNTIME_ROOT
RUNTIME_VERSION=2
TOYBOX_BIN=/system/bin/toybox
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id

[ "$#" -eq 0 ] || exit 2
[ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || exit 1
owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || exit 1
case "$owner" in 0:*) ;; *) exit 1 ;; esac

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
    version_file=$RUNTIME_ROOT/version
    if [ -L "$version_file" ] || { [ -e "$version_file" ] && [ ! -f "$version_file" ]; }; then
        return 1
    fi
    if [ -f "$version_file" ]; then
        [ "$("$TOYBOX_BIN" cat "$version_file" 2>/dev/null)" = "$RUNTIME_VERSION" ] || return 1
        [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$version_file" 2>/dev/null)" = "0:0:600" ]
        return
    fi
    temporary=$RUNTIME_ROOT/.version.new
    [ ! -e "$temporary" ] || return 1
    printf '%s\n' "$RUNTIME_VERSION" >"$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$version_file" 2>/dev/null || return 1
    [ "$("$TOYBOX_BIN" stat -c '%u:%g:%a' "$version_file" 2>/dev/null)" = "0:0:600" ]
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
    "$TOYBOX_BIN" mv "$temporary" "$pending" 2>/dev/null
}

for path in \
    "$RUNTIME_ROOT" \
    "$RUNTIME_ROOT/logs" \
    "$RUNTIME_ROOT/run" \
    "$RUNTIME_ROOT/state" \
    "$RUNTIME_ROOT/journal" \
    "$RUNTIME_ROOT/rescue-journal" \
    "$RUNTIME_ROOT/registry" \
    "$RUNTIME_ROOT/enrollment" \
    "$RUNTIME_ROOT/compatibility-policy" \
    "$RUNTIME_ROOT/enrollment-attempts" \
    "$RUNTIME_ROOT/package-state" \
    "$RUNTIME_ROOT/catalog" \
    "$RUNTIME_ROOT/slot-metadata"
do
    ensure_dir "$path" || exit 1
done
ensure_version || exit 1
prepare_boot_gate
