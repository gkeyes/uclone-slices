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
PACKAGE=$UCLONE_TARGET_PACKAGE
USER_ID=$UCLONE_TARGET_USER
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd
AM_BIN=/system/bin/am
STARTUP_GATE=$SCRIPT_DIR/startup-gate.sh
EMERGENCY_CONTAINMENT=$SCRIPT_DIR/emergency-containment.sh
JOURNAL_PACKAGES=$SCRIPT_DIR/journal-packages.sh
POST_FS_SETUP=$SCRIPT_DIR/post-fs-setup.sh

valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" |
        "$TOYBOX_BIN" grep -E -x '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' \
            >/dev/null 2>&1
}

root_has_artifact() {
    root="$1"
    if [ -L "$root" ] || { [ -e "$root" ] && [ ! -d "$root" ]; }; then
        return 0
    fi
    [ -d "$root" ] || return 1
    for entry in "$root"/*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        return 0
    done
    return 1
}
discover_generic_packages() {
    for root in \
        "$RUNTIME_ROOT/enrollment/packages" \
        "$RUNTIME_ROOT/compatibility-policy/packages" \
        "$RUNTIME_ROOT/catalog/packages" \
        "$RUNTIME_ROOT/registry/packages" \
        "$RUNTIME_ROOT/package-state/packages" \
        "$RUNTIME_ROOT/slot-metadata/packages" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts" \
        "$RUNTIME_ROOT/rescue-journal/packages" \
        "/data/misc_de/$USER_ID/$UCLONE_MODULE/slots"
    do
        [ -d "$root" ] && [ ! -L "$root" ] || continue
        for entry in "$root"/*; do
            [ -e "$entry" ] || [ -L "$entry" ] || continue
            candidate=${entry##*/}
            valid_package "$candidate" && printf '%s\n' "$candidate"
        done
    done
    for entry in "$RUNTIME_ROOT"/state/*.gate "$RUNTIME_ROOT"/state/.*.gate*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        candidate=${entry##*/}
        candidate=${candidate#.}
        candidate=${candidate%%.gate*}
        valid_package "$candidate" && printf '%s\n' "$candidate"
    done
    safe_module_binary "$JOURNAL_PACKAGES" || return 1
    "$JOURNAL_PACKAGES"
}
contain_builtin_once() {
    safe_module_binary "$CMD_BIN" || return 1
    safe_module_binary "$AM_BIN" || return 1
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        packages="$(discover_generic_packages)" || return 1
        packages="$(printf '%s\n' "$packages" | "$TOYBOX_BIN" sort -u)" || return 1
    else
        packages=$PACKAGE
    fi
    [ -n "$packages" ] || return 1
    result=0
    while IFS= read -r package; do
        [ -n "$package" ] || continue
        "$CMD_BIN" package disable-user --user "$USER_ID" "$package" >/dev/null 2>&1 || result=1
        "$AM_BIN" force-stop --user "$USER_ID" "$package" >/dev/null 2>&1 || result=1
        package_is_disabled "$package" || result=1
        package_is_quiesced "$package" || result=1
    done <<EOF
$packages
EOF
    [ "$result" -eq 0 ]
}
package_is_disabled() {
    disabled="$("$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" || return 1
    printf '%s\n' "$disabled" |
        "$TOYBOX_BIN" grep -F -x "package:$1" >/dev/null 2>&1
}
package_is_quiesced() {
    processes="$("$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    while IFS= read -r process; do
        case "$process" in "$1"|"$1":*) return 1 ;; esac
    done <<EOF
$processes
EOF
    return 0
}
launch_builtin_containment() {
    status=0
    contain_builtin_once || status=1
    (
        while management_artifact_present; do
            contain_builtin_once || :
            "$TOYBOX_BIN" sleep 2 2>/dev/null || /system/bin/sleep 2 2>/dev/null || exit 1
        done
    ) &
    return "$status"
}
management_artifact_present() {
    if [ -L "$RUNTIME_ROOT" ] || { [ -e "$RUNTIME_ROOT" ] && [ ! -d "$RUNTIME_ROOT" ]; }; then
        return 0
    fi
    [ -d "$RUNTIME_ROOT" ] || return 1
    root_has_artifact "$RUNTIME_ROOT/journal/transactions" && return 0
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        for root in \
            "$RUNTIME_ROOT/enrollment/packages" \
            "$RUNTIME_ROOT/compatibility-policy/packages" \
            "$RUNTIME_ROOT/catalog/packages" \
            "$RUNTIME_ROOT/registry/packages" \
            "$RUNTIME_ROOT/package-state/packages" \
            "$RUNTIME_ROOT/slot-metadata/packages" \
            "$RUNTIME_ROOT/enrollment-attempts/attempts" \
            "$RUNTIME_ROOT/rescue-journal/packages" \
            "/data/misc_de/$USER_ID/$UCLONE_MODULE/slots"
        do
            [ -d "$root" ] && [ ! -L "$root" ] || continue
            for entry in "$root"/*; do
                [ -e "$entry" ] || [ -L "$entry" ] || continue
                return 0
            done
        done
        for path in "$RUNTIME_ROOT"/state/*.gate "$RUNTIME_ROOT"/state/.*.gate*; do
            [ -e "$path" ] || [ -L "$path" ] || continue
            return 0
        done
        return 1
    fi
    for path in \
        "$RUNTIME_ROOT/enrollment/packages/$PACKAGE" \
        "$RUNTIME_ROOT/compatibility-policy/packages/$PACKAGE" \
        "$RUNTIME_ROOT/catalog/packages/$PACKAGE" \
        "$RUNTIME_ROOT/registry/packages/$PACKAGE" \
        "$RUNTIME_ROOT/package-state/packages/$PACKAGE" \
        "$RUNTIME_ROOT/slot-metadata/packages/$PACKAGE" \
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
    status=0
    if safe_toybox && safe_module_binary "$EMERGENCY_CONTAINMENT"; then
        "$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1 || status=1
        ("$EMERGENCY_CONTAINMENT" --watch >/dev/null 2>&1) &
    elif management_artifact_present; then
        launch_builtin_containment || status=1
    fi
    return "$status"
}
fail_closed() {
    launch_emergency_containment || :
    exit 0
}

safe_toybox() {
    [ -f "$TOYBOX_BIN" ] || return 1
    [ ! -L "$TOYBOX_BIN" ] || return 1
    [ -x "$TOYBOX_BIN" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    case "$owner" in 0:*) return 0 ;; *) return 1 ;; esac
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

safe_toybox || fail_closed
safe_module_binary "$EMERGENCY_CONTAINMENT" || fail_closed
safe_module_binary "$STARTUP_GATE" || fail_closed
safe_module_binary "$POST_FS_SETUP" || fail_closed
"$POST_FS_SETUP" || fail_closed
"$EMERGENCY_CONTAINMENT" --once >/dev/null 2>&1 || fail_closed
("$STARTUP_GATE" >/dev/null 2>&1) &

exit 0
