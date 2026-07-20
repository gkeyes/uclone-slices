#!/usr/bin/env bash

set -euo pipefail

PROFILE=slotprobe
if [ "$#" -eq 2 ] && [ "$1" = --profile ]; then
    PROFILE=$2
elif [ "$#" -ne 0 ]; then
    printf '%s\n' 'usage: tools/test-emergency-containment.sh [--profile slotprobe|fitness]' >&2
    exit 2
fi
case "$PROFILE" in
    slotprobe|fitness) ;;
    *) printf 'invalid target profile: %s\n' "$PROFILE" >&2; exit 2 ;;
esac

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
SOURCE="$REPO_ROOT/slot-kernelsu/emergency-containment.sh"
RENDERER="$REPO_ROOT/tools/render-target-profile.sh"
TMP_BASE=${TMPDIR:-/tmp}
FIXTURE=$(mktemp -d "$TMP_BASE/uclone-containment.XXXXXX")
BIN="$FIXTURE/bin"
STATE="$FIXTURE/state"
RUNTIME="$FIXTURE/runtime"
SCRIPT="$FIXTURE/emergency-containment.sh"
PROFILE_ROOT="$FIXTURE/profile"
DISABLE_LOG="$STATE/disable.log"
WATCH_PID=

fail() {
    printf 'emergency containment test failed: %s\n' "$1" >&2
    exit 1
}

cleanup() {
    status=$?
    if [ -n "${WATCH_PID:-}" ] && kill -0 "$WATCH_PID" 2>/dev/null; then
        kill "$WATCH_PID" 2>/dev/null || true
        wait "$WATCH_PID" 2>/dev/null || true
    fi
    printf '%s\n' "fixture=$FIXTURE"
    if [ "${KEEP_FIXTURE:-0}" != 1 ] && [ -d "$FIXTURE" ]; then
        rm -rf "$FIXTURE"
    fi
    if [ -e "$FIXTURE" ]; then
        printf '%s\n' 'fixture_removed=0' >&2
    else
        printf '%s\n' 'fixture_removed=1'
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

[ -f "$SOURCE" ] || fail 'missing emergency containment source'
[ -x "$RENDERER" ] || fail 'missing target profile renderer'
mkdir -p "$BIN" "$STATE" \
    "$RUNTIME/enrollment/packages" "$RUNTIME/catalog/packages" "$PROFILE_ROOT"
"$RENDERER" "$PROFILE" "$PROFILE_ROOT" >/dev/null
RENDERED="$PROFILE_ROOT/target-profile-$PROFILE"
PACKAGE=$(sed -n 's/^package=//p' "$RENDERED/target-profile.properties")
[ -n "$PACKAGE" ] || fail 'rendered target package is missing'
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$RUNTIME'|" \
    "$RENDERED/target-profile.sh" >"$FIXTURE/target-profile.sh"
chmod 444 "$FIXTURE/target-profile.sh"
touch "$RUNTIME/enrollment/packages/$PACKAGE" \
    "$RUNTIME/catalog/packages/$PACKAGE" \
    "$STATE/process-running"

cat >"$BIN/toybox" <<'EOF'
#!/bin/sh
STATE=__STATE__
case "${1:-}" in
    stat)
        case "${2:-}:${3:-}" in
            -c:%u:%g) printf '%s\n' '0:0' ;;
            -c:%a) printf '%s\n' '755' ;;
            *) exit 1 ;;
        esac
        ;;
    grep)
        shift
        exec /usr/bin/grep "$@"
        ;;
    ps)
        if [ -e "$STATE/first-ps" ]; then
            if [ -e "$STATE/process-running" ]; then
                printf '%s\n' '__PACKAGE__'
            fi
        else
            : >"$STATE/first-ps"
            : >"$STATE/quiescence-verified"
            while [ ! -e "$STATE/release-first-ps" ]; do /bin/sleep 0.02; done
        fi
        ;;
    sleep)
        shift
        exec /bin/sleep "$@"
        ;;
    *) exit 1 ;;
esac
EOF
cat >"$BIN/cmd" <<'EOF'
#!/bin/sh
STATE=__STATE__
LOG=__DISABLE_LOG__
if [ "${1:-}" = package ] && [ "${2:-}" = list ]; then
    if [ -e "$STATE/disabled" ]; then
        printf '%s\n' 'package:__PACKAGE__'
    fi
    exit 0
fi
if [ "${1:-}" = package ] && [ "${2:-}" = disable-user ]; then
    printf '%s\n' disable-user >>"$LOG"
    : >"$STATE/disabled"
    exit 0
fi
exit 1
EOF
cat >"$BIN/am" <<'EOF'
#!/bin/sh
STATE=__STATE__
if [ "${1:-}" = force-stop ]; then
    printf '%s\n' force-stop >>"$STATE/am.log"
    rm -f "$STATE/process-running"
    exit 0
fi
exit 1
EOF
chmod 755 "$BIN/toybox" "$BIN/cmd" "$BIN/am"
for fake in "$BIN/toybox" "$BIN/cmd" "$BIN/am"; do
    sed \
        -e "s|__STATE__|$STATE|g" \
        -e "s|__DISABLE_LOG__|$DISABLE_LOG|g" \
        -e "s|__PACKAGE__|$PACKAGE|g" \
        "$fake" >"$fake.tmp"
    mv "$fake.tmp" "$fake"
    chmod 755 "$fake"
done

sed \
    -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN=\"$BIN/toybox\"|" \
    -e "s|^CMD_BIN=.*$|CMD_BIN=\"$BIN/cmd\"|" \
    -e "s|^AM_BIN=.*$|AM_BIN=\"$BIN/am\"|" \
    "$SOURCE" | sed \
    -e "s|STATE=__STATE__|STATE=\"$STATE\"|g" \
    -e "s|LOG=__DISABLE_LOG__|LOG=\"$DISABLE_LOG\"|g" \
    >"$SCRIPT"
chmod 755 "$SCRIPT"

wait_for_file() {
    file="$1"
    i=0
    while [ "$i" -lt 250 ]; do
        [ -e "$file" ] && return 0
        /bin/sleep 0.02
        i=$((i + 1))
    done
    fail "timed out waiting for $(basename "$file")"
}

printf '%s\n' 'scenario=watch-recontains-after-package-reenable'
printf '%s\n' "profile=$PROFILE package=$PACKAGE"
printf '%s\n' "source=$SOURCE"
bash "$SCRIPT" --watch >"$FIXTURE/watcher.log" 2>&1 &
WATCH_PID=$!

wait_for_file "$STATE/quiescence-verified"
printf '%s\n' 'first_containment=proved'
rm -f "$STATE/disabled"
: >"$STATE/process-running"
: >"$STATE/release-first-ps"

for i in $(seq 1 250); do
    [ -f "$DISABLE_LOG" ] && [ "$(wc -l <"$DISABLE_LOG" | tr -d ' ')" -ge 2 ] && break
    /bin/sleep 0.02
done
[ -f "$DISABLE_LOG" ] || fail 'disable invocation log was not created'
disable_count=$(wc -l <"$DISABLE_LOG" | tr -d ' ')
[ "$disable_count" -ge 2 ] || fail "watch exited after first containment (disable_count=$disable_count)"
printf '%s\n' "recontainment=proved disable_count=$disable_count"

rm -f "$RUNTIME/enrollment/packages/$PACKAGE"
/bin/sleep 0.15
kill -0 "$WATCH_PID" 2>/dev/null || fail 'watch retired while a management anchor remained'
rm -f "$RUNTIME/catalog/packages/$PACKAGE"
wait "$WATCH_PID"
WATCH_PID=
printf '%s\n' 'retirement=proved all_anchors_removed'
printf '%s\n' "disable_log=$(tr '\n' ',' <"$DISABLE_LOG")"
