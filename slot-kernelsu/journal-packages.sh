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
TRANSACTIONS=$RUNTIME_ROOT/journal/transactions

[ "$#" -eq 0 ] || exit 2
[ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || exit 1
owner="$("$TOYBOX_BIN" stat -c '%u:%g' "$TOYBOX_BIN" 2>/dev/null)" || exit 1
case "$owner" in 0:*) ;; *) exit 1 ;; esac
[ -d "$TRANSACTIONS" ] && [ ! -L "$TRANSACTIONS" ] || exit 0

valid_package() {
    [ "${#1}" -le 255 ] || return 1
    printf '%s\n' "$1" |
        "$TOYBOX_BIN" grep -E -x '[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+' \
            >/dev/null 2>&1
}

safe_root_file() {
    path="$1"
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    owner="$("$TOYBOX_BIN" stat -c '%u' "$path" 2>/dev/null)" || return 1
    [ "$owner" = 0 ] || return 1
    mode="$("$TOYBOX_BIN" stat -c '%a' "$path" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

for transaction in "$TRANSACTIONS"/* "$TRANSACTIONS"/.*; do
    [ -e "$transaction" ] || [ -L "$transaction" ] || continue
    name=${transaction##*/}
    case "$name" in .|..) continue ;; esac
    [ -d "$transaction" ] && [ ! -L "$transaction" ] || exit 1
    steps=$transaction/steps
    [ -d "$steps" ] && [ ! -L "$steps" ] || exit 1
    first=$steps/0000000000000001.json
    safe_root_file "$first" || exit 1
    size="$("$TOYBOX_BIN" wc -c <"$first" 2>/dev/null |
        "$TOYBOX_BIN" tr -d '[:space:]')" || exit 1
    case "$size" in *[!0-9]*|'') exit 1 ;; esac
    [ "$size" -le 65536 ] || exit 1
    count="$("$TOYBOX_BIN" grep -o '"package_name"' "$first" 2>/dev/null |
        "$TOYBOX_BIN" wc -l | "$TOYBOX_BIN" tr -d '[:space:]')" || exit 1
    [ "$count" = 1 ] || exit 1
    package="$("$TOYBOX_BIN" sed -n \
        's/.*"package_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        "$first")" || exit 1
    valid_package "$package" || exit 1
    printf '%s\n' "$package"
done
