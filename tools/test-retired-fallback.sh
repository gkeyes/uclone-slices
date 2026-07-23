#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-retired-fallback.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
BIN="$FIXTURE/bin"
STATE="$FIXTURE/state"
RUNTIME="$FIXTURE/runtime"
PACKAGE=com.example.retired
mkdir -p "$BIN" "$STATE" \
    "$RUNTIME/enrollment/packages/$PACKAGE" "$RUNTIME/catalog/packages/$PACKAGE" \
    "$RUNTIME/rescue-journal/packages/$PACKAGE/rescue/steps" \
    "$RUNTIME/journal/transactions/$PACKAGE/steps" "$FIXTURE/profile"
"$ROOT/tools/render-target-profile.sh" generic "$FIXTURE/profile" >/dev/null
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$FIXTURE/profile/target-profile-generic/target-profile.sh" >"$FIXTURE/target-profile.sh"

write_rescue_step() {
    generation="$1"
    event="$2"
    if [ "$generation" -eq 1 ]; then
        unsigned='{"schema_version":1,"rescue_id":"rescue-00000001","generation":1,"previous_sha256":null,"event":{"type":"prepared","spec":{"package_key":{"package_name":"com.example.retired","user_id":0}}}}'
    else
        unsigned="{\"schema_version\":1,\"rescue_id\":\"rescue-00000001\",\"generation\":$generation,\"previous_sha256\":\"$PREVIOUS\",\"event\":{\"type\":\"$event\"}}"
    fi
    digest=$(printf '%s' "$unsigned" | shasum -a 256 | awk '{print $1}')
    printf '%s,"sha256":"%s"}' "${unsigned%\}}" "$digest" \
        >"$RUNTIME/rescue-journal/packages/$PACKAGE/rescue/steps/$(printf '%016d.json' "$generation")"
    PREVIOUS=$digest
}
PREVIOUS=
write_rescue_step 1 prepared
write_rescue_step 2 gate_held
write_rescue_step 3 processes_quiesced
write_rescue_step 4 base_applying
write_rescue_step 5 base_verified
write_rescue_step 6 base_committed
write_rescue_step 7 gate_released
write_rescue_step 8 completed
printf '%s\n' '{"package_name":"com.example.retired","event":{"type":"prepared"}}' \
    >"$RUNTIME/journal/transactions/$PACKAGE/steps/0000000000000001.json"

cat >"$BIN/toybox" <<'EOF'
#!/bin/sh
case "${1:-}" in
    stat)
        case "${2:-}:${3:-}" in
            -c:%u:%g) printf '%s\n' '0:0' ;;
            -c:%u) printf '%s\n' '0' ;;
            -c:%a) printf '%s\n' '755' ;;
            *) exit 1 ;;
        esac ;;
    grep|sed|sort|tr|wc) tool=$1; shift; exec "/usr/bin/$tool" "$@" ;;
    sha256sum) shift; exec /usr/bin/shasum -a 256 "$@" ;;
    ps) exit 0 ;;
    timeout) shift; [ "${1:-}" = -s ] && shift 2; shift; exec "$@" ;;
    sleep) exit 0 ;;
    *) exit 1 ;;
esac
EOF
cat >"$BIN/cmd" <<EOF
#!/bin/sh
[ "\${1:-}:\${2:-}" != package:list ] || {
    [ ! -e '$STATE/disabled' ] || printf '%s\n' 'package:$PACKAGE'
    exit 0
}
printf '%s\n' "\$*" >>'$STATE/cmd.log'
: >'$STATE/disabled'
exit 0
EOF
cat >"$BIN/am" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >>'$STATE/am.log'
exit 0
EOF
chmod 755 "$BIN/toybox" "$BIN/cmd" "$BIN/am"
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/rescue-retired-packages.sh" >"$FIXTURE/rescue-retired-packages.sh"
chmod 755 "$FIXTURE/rescue-retired-packages.sh"
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/journal-packages.sh" >"$FIXTURE/journal-packages.sh"
chmod 755 "$FIXTURE/journal-packages.sh"
sed -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^CMD_BIN=.*$|CMD_BIN='$BIN/cmd'|" \
    -e "s|^AM_BIN=.*$|AM_BIN='$BIN/am'|" \
    -e "s|^RESCUE_PACKAGES=.*$|RESCUE_PACKAGES='$FIXTURE/rescue-retired-packages.sh'|" \
    -e "s|^JOURNAL_PACKAGES=.*$|JOURNAL_PACKAGES='$FIXTURE/journal-packages.sh'|" \
    -e "s|^RUNTIME_BIN=.*$|RUNTIME_BIN='$BIN/missing-ucloned'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/emergency-containment.sh" >"$FIXTURE/emergency.sh"
chmod 755 "$FIXTURE/emergency.sh"

result="$(/bin/sh "$FIXTURE/emergency.sh" --once 2>/dev/null)"
[ "$result" = held ]
[ -s "$STATE/cmd.log" ] && [ -s "$STATE/am.log" ]
: >"$STATE/runtime-unavailable-covered"

rm -f "$FIXTURE/rescue-retired-packages.sh"
ln -s "$ROOT/slot-kernelsu/rescue-retired-packages.sh" "$FIXTURE/rescue-retired-packages.sh"
set +e
result="$(/bin/sh "$FIXTURE/emergency.sh" --once 2>/dev/null)"
status=$?
set -e
[ "$status" -ne 0 ] && [ "$result" = recovery-required ]
rm "$FIXTURE/rescue-retired-packages.sh"
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/rescue-retired-packages.sh" >"$FIXTURE/rescue-retired-packages.sh"
chmod 755 "$FIXTURE/rescue-retired-packages.sh"

mkdir "$RUNTIME/enrollment/packages/not-a-package"
set +e
result="$(/bin/sh "$FIXTURE/emergency.sh" --once 2>/dev/null)"
status=$?
set -e
[ "$status" -ne 0 ] && [ "$result" = recovery-required ]
! grep -F 'not-a-package' "$STATE/cmd.log" >/dev/null
! grep -F 'not-a-package' "$STATE/am.log" >/dev/null
printf '%s\n' 'Unavailable Runtime contains strong anchors and rejects corrupt package metadata.'
