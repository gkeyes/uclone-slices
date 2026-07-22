#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-generic-containment.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
BIN="$FIXTURE/bin"
STATE="$FIXTURE/state"
RUNTIME="$FIXTURE/runtime"
mkdir -p "$BIN" "$STATE" "$RUNTIME/enrollment/packages" \
    "$RUNTIME/journal/transactions/journal-only/steps" "$FIXTURE/profile"
"$ROOT/tools/render-target-profile.sh" generic "$FIXTURE/profile" >/dev/null
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$FIXTURE/profile/target-profile-generic/target-profile.sh" \
    >"$FIXTURE/target-profile.sh"
for package in com.asksky.fitness com.uclone.slotprobe; do
    mkdir "$RUNTIME/enrollment/packages/$package"
    : >"$STATE/process-$package"
done
JOURNAL_PACKAGE=com.example.journal
: >"$STATE/process-$JOURNAL_PACKAGE"
cat >"$RUNTIME/journal/transactions/journal-only/steps/0000000000000001.json" <<EOF
{"event":{"prepared":{"spec":{"managed_package":{"package_name":"$JOURNAL_PACKAGE"}}}}}
EOF
chmod 600 "$RUNTIME/journal/transactions/journal-only/steps/0000000000000001.json"

cat >"$BIN/toybox" <<EOF
#!/bin/sh
case "\${1:-}" in
    stat)
        case "\${2:-}:\${3:-}" in
            -c:%u:%g) printf '%s\n' '0:0' ;;
            -c:%u) printf '%s\n' '0' ;;
            -c:%a) printf '%s\n' '755' ;;
            *) exit 1 ;;
        esac ;;
    grep) shift; exec /usr/bin/grep "\$@" ;;
    sed) shift; exec /usr/bin/sed "\$@" ;;
    wc) shift; exec /usr/bin/wc "\$@" ;;
    sort) shift; exec /usr/bin/sort "\$@" ;;
    tr) shift; exec /usr/bin/tr "\$@" ;;
    ps)
        for path in '$STATE'/process-*; do
            [ -e "\$path" ] && printf '%s\n' "\${path##*-}"
        done
        exit 0 ;;
    *) exit 1 ;;
esac
EOF
cat >"$BIN/cmd" <<EOF
#!/bin/sh
if [ "\${1:-}:\${2:-}" = package:list ]; then
    for path in '$STATE'/disabled-*; do
        [ -e "\$path" ] && printf 'package:%s\n' "\${path##*-}"
    done
    exit 0
fi
if [ "\${1:-}:\${2:-}" = package:disable-user ]; then
    package="\${5:-}"
    : >'$STATE'/disabled-"\$package"
    printf '%s\n' "\$package" >>'$STATE/disable.log'
    exit 0
fi
exit 1
EOF
cat >"$BIN/am" <<EOF
#!/bin/sh
[ "\${1:-}" = force-stop ] || exit 1
package="\${4:-}"
rm -f '$STATE'/process-"\$package"
exit 0
EOF
chmod 755 "$BIN/toybox" "$BIN/cmd" "$BIN/am"
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/journal-packages.sh" >"$FIXTURE/journal-packages.sh"
chmod 755 "$FIXTURE/journal-packages.sh"
sed -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^CMD_BIN=.*$|CMD_BIN='$BIN/cmd'|" \
    -e "s|^AM_BIN=.*$|AM_BIN='$BIN/am'|" \
    -e "s|^RUNTIME_BIN=.*$|RUNTIME_BIN='$BIN/missing-ucloned'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/emergency-containment.sh" >"$FIXTURE/emergency.sh"
chmod 755 "$FIXTURE/emergency.sh"

/bin/sh "$FIXTURE/emergency.sh" --once >/dev/null
sort -u "$STATE/disable.log" >"$STATE/actual"
printf '%s\n' com.asksky.fitness com.uclone.slotprobe "$JOURNAL_PACKAGE" | sort >"$STATE/expected"
cmp "$STATE/expected" "$STATE/actual"
printf '%s\n' 'Generic emergency fallback contained every discovered managed package.'
