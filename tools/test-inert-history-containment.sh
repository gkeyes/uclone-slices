#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-inert-history.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
BIN="$FIXTURE/bin"
STATE="$FIXTURE/state"
RUNTIME="$FIXTURE/runtime"
RETIRED=com.example.retired
mkdir -p "$BIN" "$STATE" "$RUNTIME/state" \
    "$RUNTIME/enrollment/packages/$RETIRED" \
    "$RUNTIME/rescue-journal/packages/$RETIRED/rescue/steps" \
    "$RUNTIME/journal/transactions/$RETIRED/steps" "$FIXTURE/profile"
"$ROOT/tools/render-target-profile.sh" generic "$FIXTURE/profile" >/dev/null
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$FIXTURE/profile/target-profile-generic/target-profile.sh" \
    >"$FIXTURE/target-profile.sh"
cat >"$RUNTIME/state/.$RETIRED.gate.retired" <<EOF
package=$RETIRED
user_id=0
enabled_state=default
suspended=false
base_ce_inode=101
base_de_inode=102
EOF

RESCUE_STEPS="$RUNTIME/rescue-journal/packages/$RETIRED/rescue/steps"
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
        >"$RESCUE_STEPS/$(printf '%016d.json' "$generation")"
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
    >"$RUNTIME/journal/transactions/$RETIRED/steps/0000000000000001.json"

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
    sha256sum) shift; exec /usr/bin/shasum -a 256 "\$@" ;;
    ps) exit 0 ;;
    timeout)
        shift
        [ "\${1:-}" = -s ] && shift 2
        shift
        exec "\$@"
        ;;
    nsenter)
        shift
        while [ "\${1:-}" != -- ]; do shift; done
        shift
        exec "\$@"
        ;;
    sleep) exit 0 ;;
    *) exit 1 ;;
esac
EOF
cat >"$BIN/cmd" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >>'$STATE/cmd.log'
exit 1
EOF
cat >"$BIN/am" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >>'$STATE/am.log'
exit 1
EOF
cat >"$BIN/ucloned" <<EOF
#!/bin/sh
: >'$STATE/runtime-called'
printf '%s\n' not-managed
EOF
chmod 755 "$BIN/toybox" "$BIN/cmd" "$BIN/am" "$BIN/ucloned"
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/journal-packages.sh" >"$FIXTURE/journal-packages.sh"
chmod 755 "$FIXTURE/journal-packages.sh"
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
    -e "s|^RUNTIME_BIN=.*$|RUNTIME_BIN='$BIN/ucloned'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/emergency-containment.sh" >"$FIXTURE/emergency.sh"
chmod 755 "$FIXTURE/emergency.sh"

result="$(/bin/sh "$FIXTURE/emergency.sh" --once)"
[ "$result" = not-managed ]
[ -e "$STATE/runtime-called" ]
[ ! -e "$STATE/cmd.log" ]
[ ! -e "$STATE/am.log" ]
rm -f "$STATE/runtime-called"
/bin/sh "$FIXTURE/emergency.sh" --watch &
watch_pid=$!
for _ in $(seq 1 500); do
    kill -0 "$watch_pid" 2>/dev/null || break
    /bin/sleep 0.02
done
if kill -0 "$watch_pid" 2>/dev/null; then
    kill "$watch_pid" 2>/dev/null || true
    wait "$watch_pid" 2>/dev/null || true
    printf '%s\n' 'typed BaseRetired watcher did not exit' >&2
    exit 1
fi
wait "$watch_pid"
[ -e "$STATE/runtime-called" ]
[ ! -e "$STATE/cmd.log" ]
[ ! -e "$STATE/am.log" ]
printf '%s\n' 'Typed BaseRetired state supersedes stale anchors without containment.'
