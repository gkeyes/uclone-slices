#!/system/bin/sh
set -u
umask 077
case "$0" in
    /*/*) SCRIPT_DIR=${0%/*} ;;
    *) exit 1 ;;
esac
PROFILE_FILE=$SCRIPT_DIR/target-profile.sh
PROFILE_LOADER=$SCRIPT_DIR/profile-loader.sh
UCLONE_TARGET_PROFILE=generic
UCLONE_TARGET_USER=0
UCLONE_MODULE=uclone-slices-preview
UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview
if [ -f "$PROFILE_LOADER" ] && [ ! -L "$PROFILE_LOADER" ]; then
    . "$PROFILE_LOADER"
    uclone_load_profile "$PROFILE_FILE" || exit 1
fi
RUNTIME_ROOT=$UCLONE_RUNTIME_ROOT
TOYBOX_BIN=/system/bin/toybox
JOURNAL_PACKAGES=$SCRIPT_DIR/journal-packages.sh

safe_tool() {
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$($TOYBOX_BIN stat -c '%a' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

safe_dir() {
    path=$1
    [ -d "$path" ] && [ ! -L "$path" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u:%g' "$path" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$($TOYBOX_BIN stat -c '%a' "$path" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

safe_file() {
    path=$1
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u:%g' "$path" 2>/dev/null)" || return 1
    case "$owner" in 0:*) ;; *) return 1 ;; esac
    mode="$($TOYBOX_BIN stat -c '%a' "$path" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

safe_executable() {
    safe_file "$1" && [ -x "$1" ]
}

valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" | "$TOYBOX_BIN" grep -E -x \
        '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' >/dev/null 2>&1
}

emit_package() {
    package=$1
    valid_package "$package" || return 1
    printf '%s\n' "$package"
}

scan_package_root() {
    root=$1
    [ -e "$root" ] || [ -L "$root" ] || return 0
    safe_dir "$root" || return 1
    for entry in "$root"/* "$root"/.[!.]* "$root"/..?*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        package=${entry##*/}
        safe_dir "$entry" || safe_file "$entry" || return 1
        emit_package "$package" || return 1
    done
}

scan_state_root() {
    root=$RUNTIME_ROOT/state
    [ -e "$root" ] || [ -L "$root" ] || return 0
    safe_dir "$root" || return 1
    for entry in "$root"/* "$root"/.[!.]* "$root"/..?*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        name=${entry##*/}
        case "$name" in
            startup-gate.pending|startup-gate.ready|emergency-containment.request)
                safe_file "$entry" || return 1
                continue
                ;;
            .*.gate.retired)
                safe_file "$entry" || return 1
                continue
                ;;
            *.gate|.*.gate|.*.gate.*)
                safe_file "$entry" || return 1
                package=${name#.}
                package=${package%%.gate*}
                emit_package "$package" || return 1
                ;;
            *)
                return 1
                ;;
        esac
    done
}

scan_journal() {
    root=$RUNTIME_ROOT/journal/transactions
    [ -e "$root" ] || [ -L "$root" ] || return 0
    safe_dir "$root" || return 1
    safe_executable "$JOURNAL_PACKAGES" || return 1
    "$JOURNAL_PACKAGES"
}

scan_control() {
    [ -e "$RUNTIME_ROOT" ] || [ -L "$RUNTIME_ROOT" ] || return 0
    safe_dir "$RUNTIME_ROOT" || return 1
    for root in \
        "$RUNTIME_ROOT/enrollment/packages" \
        "$RUNTIME_ROOT/compatibility-policy/packages" \
        "$RUNTIME_ROOT/catalog/packages" \
        "$RUNTIME_ROOT/registry/packages" \
        "$RUNTIME_ROOT/package-state/packages" \
        "$RUNTIME_ROOT/slot-metadata/packages" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts" \
        "$RUNTIME_ROOT/rescue-journal/packages" \
        "/data/misc_de/$UCLONE_TARGET_USER/$UCLONE_MODULE/slots"
    do
        scan_package_root "$root" || return 1
    done
    scan_state_root || return 1
    scan_journal
}

[ "$#" -eq 1 ] || exit 2
safe_tool || exit 1
case "$1" in
    --active-control|--retired|--active) scan_control ;;
    *) exit 2 ;;
esac
