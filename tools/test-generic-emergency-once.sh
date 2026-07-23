#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-generic-containment.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
BIN="$FIXTURE/bin"
STATE="$FIXTURE/state"
RUNTIME="$FIXTURE/runtime"
mkdir -p "$BIN" "$STATE" "$RUNTIME/enrollment/packages" \
    "$RUNTIME/journal/transactions/journal-only/steps" \
    "$RUNTIME/journal/transactions/completed/steps" "$RUNTIME/state" "$FIXTURE/profile"
"$ROOT/tools/render-target-profile.sh" generic "$FIXTURE/profile" >/dev/null
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$FIXTURE/profile/target-profile-generic/target-profile.sh" \
    >"$FIXTURE/target-profile.sh"
for package in com.asksky.fitness com.uclone.slotprobe; do
    mkdir "$RUNTIME/enrollment/packages/$package"
    : >"$STATE/process-$package"
done
JOURNAL_PACKAGE=com.example.journal
PREPARED_PACKAGE=com.example.prepared
COMPLETED_PACKAGE=com.example.completed
RETIRED_PACKAGE=com.example.retired
: >"$STATE/process-$JOURNAL_PACKAGE"
: >"$STATE/process-$PREPARED_PACKAGE"
: >"$STATE/process-$COMPLETED_PACKAGE"
: >"$STATE/process-$RETIRED_PACKAGE"
cat >"$RUNTIME/state/$PREPARED_PACKAGE.gate" <<EOF
package=$PREPARED_PACKAGE
user_id=0
enabled_state=default
suspended=false
phase=prepared
base_ce_inode=0
base_de_inode=0
EOF
cat >"$RUNTIME/state/.$RETIRED_PACKAGE.gate.retired" <<EOF
package=$RETIRED_PACKAGE
user_id=0
enabled_state=default
suspended=false
base_ce_inode=101
base_de_inode=102
EOF
cat >"$RUNTIME/journal/transactions/journal-only/steps/0000000000000001.json" <<EOF
{"event":{"prepared":{"spec":{"managed_package":{"package_name":"$JOURNAL_PACKAGE"}}}}}
EOF
chmod 600 "$RUNTIME/journal/transactions/journal-only/steps/0000000000000001.json"
cat >"$RUNTIME/journal/transactions/completed/steps/0000000000000001.json" <<EOF
{"event":{"prepared":{"spec":{"managed_package":{"package_name":"$COMPLETED_PACKAGE"}}}}}
EOF
printf '%s\n' '{"event":{"type":"completed"}}' \
    >"$RUNTIME/journal/transactions/completed/steps/0000000000000002.json"
chmod 600 "$RUNTIME/journal/transactions/completed/steps/"*.json

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
    timeout)
        shift
        if [ "\${1:-}" = -s ]; then shift 2; fi
        seconds=\${1:-}
        [ -n "\$seconds" ] || exit 2
        shift
        "\$@" &
        child=\$!
        ticks=\$((seconds * 100))
        tick=0
        while [ "\$tick" -lt "\$ticks" ]; do
            child_state=\$(/bin/ps -p "\$child" -o state= 2>/dev/null) || break
            case "\$child_state" in *Z*) break ;; esac
            /bin/sleep 0.01
            tick=\$((tick + 1))
        done
        kill -9 "\$child" 2>/dev/null || :
        status=0
        wait "\$child" 2>/dev/null || status=\$?
        exit "\$status" ;;
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
sed -e '1s|.*|#!/bin/sh|' \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/rescue-retired-packages.sh" >"$FIXTURE/rescue-retired-packages.sh"
chmod 755 "$FIXTURE/rescue-retired-packages.sh"
sed -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
    -e "s|^CMD_BIN=.*$|CMD_BIN='$BIN/cmd'|" \
    -e "s|^AM_BIN=.*$|AM_BIN='$BIN/am'|" \
    -e "s|^RESCUE_PACKAGES=.*$|RESCUE_PACKAGES='$FIXTURE/rescue-retired-packages.sh'|" \
    -e "s|^RUNTIME_BIN=.*$|RUNTIME_BIN='$BIN/missing-ucloned'|" \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$ROOT/slot-kernelsu/emergency-containment.sh" >"$FIXTURE/emergency.sh"
chmod 755 "$FIXTURE/emergency.sh"

/bin/sh "$FIXTURE/emergency.sh" --once >/dev/null
sort -u "$STATE/disable.log" >"$STATE/actual"
printf '%s\n' com.asksky.fitness com.uclone.slotprobe \
    "$JOURNAL_PACKAGE" "$PREPARED_PACKAGE" | sort >"$STATE/expected"
cmp "$STATE/expected" "$STATE/actual"
[ ! -e "$STATE/disabled-$COMPLETED_PACKAGE" ]
[ ! -e "$STATE/disabled-$RETIRED_PACKAGE" ]
[ -e "$STATE/process-$COMPLETED_PACKAGE" ]
[ -e "$STATE/process-$RETIRED_PACKAGE" ]

for package in com.asksky.fitness com.uclone.slotprobe; do
    rmdir "$RUNTIME/enrollment/packages/$package"
done
rm -f "$RUNTIME/journal/transactions/journal-only/steps/0000000000000001.json" \
    "$RUNTIME/state/$PREPARED_PACKAGE.gate"
rmdir "$RUNTIME/journal/transactions/journal-only/steps" \
    "$RUNTIME/journal/transactions/journal-only"
before=$(wc -l <"$STATE/disable.log" | tr -d ' ')
result="$(/bin/sh "$FIXTURE/emergency.sh" --once)"
[ "$result" = not-managed ]
after=$(wc -l <"$STATE/disable.log" | tr -d ' ')
[ "$before" = "$after" ]

/bin/sh "$FIXTURE/emergency.sh" --watch &
watch_pid=$!
for _ in $(seq 1 100); do
    kill -0 "$watch_pid" 2>/dev/null || break
    /bin/sleep 0.02
done
if kill -0 "$watch_pid" 2>/dev/null; then
    kill "$watch_pid" 2>/dev/null || true
    wait "$watch_pid" 2>/dev/null || true
    printf '%s\n' 'generic inert-history watcher did not retire' >&2
    exit 1
fi
wait "$watch_pid"

: >"$RUNTIME/enrollment/packages/not-a-package"
set +e
result="$(/bin/sh "$FIXTURE/emergency.sh" --once)"
status=$?
set -e
[ "$status" -ne 0 ]
[ "$result" = recovery-required ]
printf '%s\n' 'Generic containment covered live and prepared state, retired on inert history, and rejected corrupt metadata.'
