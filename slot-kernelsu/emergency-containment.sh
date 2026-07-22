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
RUNTIME_BIN=$SCRIPT_DIR/bin/ucloned
JOURNAL_PACKAGES=$SCRIPT_DIR/journal-packages.sh

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
            root_has_artifact "$root" && return 0
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
        "$RUNTIME_ROOT/state/.$PACKAGE.gate.retired" \
        "/data/misc_ce/$USER_ID/$UCLONE_MODULE/slots/$PACKAGE" \
        "/data/misc_de/$USER_ID/$UCLONE_MODULE/slots/$PACKAGE"
    do
        if [ -e "$path" ] || [ -L "$path" ]; then
            return 0
        fi
    done
    return 1
}

valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" |
        "$TOYBOX_BIN" grep -E -x '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' \
            >/dev/null 2>&1
}

discover_packages() {
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
    safe_binary "$JOURNAL_PACKAGES" || return 1
    "$JOURNAL_PACKAGES"
}

package_is_disabled() {
    package="$1"
    disabled_packages="$("$CMD_BIN" package list packages -d --user "$USER_ID" 2>/dev/null)" ||
        return 1
    printf '%s\n' "$disabled_packages" |
        "$TOYBOX_BIN" grep -F -x "package:$package" >/dev/null 2>&1
}

package_is_quiesced() {
    package="$1"
    processes="$("$TOYBOX_BIN" ps -A -o NAME 2>/dev/null)" || return 1
    while IFS= read -r process; do
        case "$process" in
            "$package"|"$package":*) return 1 ;;
        esac
    done <<EOF
$processes
EOF
    return 0
}

contain_once() {
    package="$1"
    safe_binary "$TOYBOX_BIN" || return 1
    safe_binary "$CMD_BIN" || return 1
    safe_binary "$AM_BIN" || return 1
    "$CMD_BIN" package disable-user --user "$USER_ID" "$package" >/dev/null 2>&1 || return 1
    "$AM_BIN" force-stop --user "$USER_ID" "$package" >/dev/null 2>&1 || return 1
    package_is_disabled "$package" && package_is_quiesced "$package"
}

contain_discovered() {
    packages="$(discover_packages)"
    status=$?
    [ "$status" -eq 0 ] || return 1
    [ -n "$packages" ] || return 1
    result=0
    while IFS= read -r package; do
        [ -n "$package" ] || continue
        contain_once "$package" || result=1
    done <<EOF
$packages
EOF
    [ "$result" -eq 0 ]
}

contain_with_runtime() {
    safe_binary "$RUNTIME_BIN" || return 1
    result="$("$TOYBOX_BIN" timeout -s 9 8 "$TOYBOX_BIN" nsenter -t 1 -m -- \
        "$RUNTIME_BIN" --startup-gate 2>/dev/null)" || return 1
    [ "$result" = held ]
}

attempt_once() {
    if ! management_artifact_present; then
        printf '%s\n' not-managed
        return 0
    fi
    if [ "$UCLONE_TARGET_PROFILE" = generic ]; then
        contain_discovered || contain_with_runtime || {
            printf '%s\n' recovery-required
            return 1
        }
    elif ! contain_once "$PACKAGE"; then
        printf '%s\n' recovery-required
        return 1
    fi
    if management_artifact_present; then
        printf '%s\n' held
        return 0
    fi
    printf '%s\n' not-managed
    return 0
}

case "${1:-}" in
    --once)
        [ -z "${2:-}" ] || exit 2
        attempt_once
        ;;
    --watch)
        [ -z "${2:-}" ] || exit 2
        while management_artifact_present; do
            attempt_once >/dev/null 2>&1 || :
            if safe_binary "$TOYBOX_BIN"; then
                "$TOYBOX_BIN" sleep 2
            else
                /system/bin/sleep 2 2>/dev/null || exit 1
            fi
        done
        exit 0
        ;;
    *)
        exit 2
        ;;
esac
