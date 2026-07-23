#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -eq 0 ]; then
    PROFILE=slotprobe
elif [ "$#" -eq 2 ] && [ "$1" = --profile ]; then
    PROFILE=$2
else
    printf '%s\n' 'usage: tools/test-kernelsu-boot-flow.sh [--profile slotprobe|fitness|generic]' >&2
    exit 2
fi
case "$PROFILE" in slotprobe|fitness|generic) ;; *) exit 2 ;; esac

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
KERNELSU_ROOT="$REPO_ROOT/slot-kernelsu"
STATE_HELPER="$KERNELSU_ROOT/boot-state.sh"
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/uclone-boot-profile.XXXXXX")
cleanup() {
    if [ "${KEEP_FIXTURE:-0}" = 1 ]; then
        printf 'fixture=%s\n' "$SCRATCH"
        return
    fi
    chmod -R u+w "$SCRATCH" 2>/dev/null || true
    rm -rf "$SCRATCH"
}
trap cleanup EXIT HUP INT TERM
"$REPO_ROOT/tools/render-target-profile.sh" "$PROFILE" "$SCRATCH" >/dev/null
. "$SCRATCH/target-profile-$PROFILE/target-profile.sh"

fail() {
    printf 'boot-flow test failed: %s\n' "$1" >&2
    exit 1
}

for script in customize.sh post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh boot-completed.sh boot-state.sh profile-loader.sh rescue.sh prepare-upgrade.sh upgrade-freeze.sh; do
    [ -f "$KERNELSU_ROOT/$script" ] || fail "missing $script"
    /bin/sh -n "$KERNELSU_ROOT/$script"
done

# The classifier is the only shell interpretation of the typed slotctl response.
# A locked result must stay pending; only a non-locked success may be terminal.
# shellcheck disable=SC1090
. "$STATE_HELPER"
if [ "$PROFILE" = generic ]; then
    complete='{"status":"ok","payload":{"kind":"ack","data":{"operation":"reconcile_all"}}}'
    [ "$(classify_reconcile_frame "$complete")" = terminal ] ||
        fail 'generic reconcile-all acknowledgement was not terminal'
    [ "$(classify_reconcile_frame '{"status":"error","error_code":"user_locked"}')" = invalid ] ||
        fail 'generic locked error was accepted as terminal'
else
    locked="{\"status\":\"ok\",\"payload\":{\"kind\":\"reconcile_report\",\"data\":{\"package\":\"$UCLONE_TARGET_PACKAGE\",\"outcome\":{\"kind\":\"locked\"}}}}"
    restored="${locked/\"locked\"/\"restored_base\"}"
    recovery="${locked/\"locked\"/\"recovery_required\"}"
    if [ "$PROFILE" = slotprobe ]; then foreign_package=com.asksky.fitness; else foreign_package=com.uclone.slotprobe; fi
    foreign="${locked/$UCLONE_TARGET_PACKAGE/$foreign_package}"
    [ "$(classify_reconcile_frame "$locked")" = locked ] || fail 'locked frame was not classified as pending'
    [ "$(classify_reconcile_frame "$restored")" = terminal ] || fail 'restored frame was not terminal'
    [ "$(classify_reconcile_frame "$recovery")" = terminal ] || fail 'recovery-required frame was not terminal'
    [ "$(classify_reconcile_frame "$foreign")" = invalid ] || fail 'foreign package frame was accepted'
fi
[ "$(classify_reconcile_frame 'not-json')" = invalid ] || fail 'invalid frame was accepted'
[ "$(reconcile_marker_action locked)" = retain ] || fail 'locked outcome would consume the marker'
[ "$(reconcile_marker_action invalid)" = retain ] || fail 'invalid outcome would consume the marker'
[ "$(reconcile_marker_action terminal)" = retire ] || fail 'terminal outcome would not retire the marker'

POST_FS="$KERNELSU_ROOT/post-fs-data.sh"
POST_FS_SETUP="$KERNELSU_ROOT/post-fs-setup.sh"
SERVICE="$KERNELSU_ROOT/service.sh"
BOOT="$KERNELSU_ROOT/boot-completed.sh"
STARTUP="$KERNELSU_ROOT/startup-gate.sh"
EMERGENCY="$KERNELSU_ROOT/emergency-containment.sh"
PACKAGER="$REPO_ROOT/tools/package-kernelsu-preview.sh"
BRIDGE_SOURCE="$REPO_ROOT/slot-runtime/src/bridge/mod.rs"
MATERIALIZER_COMMAND="$REPO_ROOT/slot-runtime/src/android/materializer/command.rs"
PACKAGE_ZIP="${UCLONE_KERNELSU_PACKAGE:-}"

version_functions="$SCRATCH/runtime-version-functions.sh"
sed -n '/^version_transition_allowed()/,/^}/p' "$POST_FS_SETUP" >"$version_functions"
[ -s "$version_functions" ] || fail 'runtime version transition policy is missing'
. "$version_functions"
version_transition_allowed 1 2 || fail 'paired Runtime v1 to v2 migration is rejected'
version_transition_allowed 2 2 || fail 'current Runtime v2 is rejected'
if version_transition_allowed 0 2 || version_transition_allowed 2 1 ||
    version_transition_allowed 3 2 || version_transition_allowed invalid 2; then
    fail 'unsupported Runtime version transition was accepted'
fi

if [ -n "$PACKAGE_ZIP" ]; then
    [ -f "$PACKAGE_ZIP" ] || fail "KernelSU package is missing: $PACKAGE_ZIP"
    package_entries="$SCRATCH/package-entries"
    package_extract="$SCRATCH/package-extract"
    mkdir -p "$package_extract"
    /usr/bin/unzip -Z1 "$PACKAGE_ZIP" | awk '!/\/$/' | sort >"$package_entries"
    grep -F -x 'disable' "$package_entries" >/dev/null || fail 'KernelSU package omits root disable marker'
    /usr/bin/unzip -qq "$PACKAGE_ZIP" disable -d "$package_extract"
    [ -f "$package_extract/disable" ] || fail 'KernelSU package disable marker did not extract as a regular file'
    [ ! -L "$package_extract/disable" ] || fail 'KernelSU package disable marker is a symlink'
    [ ! -s "$package_extract/disable" ] || fail 'KernelSU package disable marker is not zero-length'
    package_disable_mode=$(stat -f '%Lp' "$package_extract/disable" 2>/dev/null || stat -c '%a' "$package_extract/disable")
    [ "$package_disable_mode" = 644 ] || fail "KernelSU package disable marker mode is $package_disable_mode, expected 644"
    printf 'KernelSU package disable marker passed profile=%s regular=1 symlink=0 bytes=0 mode=%s.\n' \
        "$PROFILE" "$package_disable_mode"
fi

grep -F 'startup-gate.sh' "$POST_FS" >/dev/null || fail 'post-fs does not launch the early gate worker'
grep -F 'write_runtime_version "$version_file"' "$POST_FS_SETUP" >/dev/null || fail 'post-fs setup lacks atomic Runtime version publication'
grep -F '"$EMERGENCY_CONTAINMENT" --once' "$POST_FS" >/dev/null || fail 'post-fs does not synchronously prove emergency containment'
grep -F '("$STARTUP_GATE" >/dev/null 2>&1) &' "$POST_FS" >/dev/null || fail 'post-fs does not launch startup reconciliation without blocking boot'
grep -F 'emergency-containment.sh' "$POST_FS" >/dev/null || fail 'post-fs lacks the emergency containment fallback'
grep -F '"$MODPATH/post-fs-setup.sh"' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'customize does not restore post-fs setup executable mode'
grep -F '"$MODPATH/journal-packages.sh"' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'customize does not restore journal helper executable mode'
grep -F '"$MODPATH/rescue-retired-packages.sh"' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'customize does not restore rescue helper executable mode'
grep -F 'startup-gate.ready' "$SERVICE" >/dev/null || fail 'service does not require boot-scoped gate proof'
grep -F 'emergency-containment.sh' "$SERVICE" >/dev/null || fail 'service lacks the emergency containment fallback'
grep -F -- '--startup-gate' "$STARTUP" >/dev/null || fail 'startup worker does not use the exact-lease Rust path'
grep -F '"$EMERGENCY_CONTAINMENT" --once' "$STARTUP" >/dev/null || fail 'startup failure does not synchronously contain known packages'
grep -F 'emergency-containment.sh' "$STARTUP" >/dev/null || fail 'startup worker lacks the emergency containment fallback'
grep -F 'BOOT_RECONCILER' "$SERVICE" >/dev/null || fail 'service lacks redundant delayed-unlock worker'
grep -F 'case "$marker_action:$classification" in' "$BOOT" >/dev/null || fail 'reconcile worker does not apply marker policy'
grep -F 'retain:locked)' "$BOOT" >/dev/null || fail 'reconcile worker lacks locked retry branch'
grep -F 'for script in customize.sh post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh boot-completed.sh boot-state.sh profile-loader.sh rescue.sh prepare-upgrade.sh upgrade-freeze.sh' "$PACKAGER" >/dev/null || fail 'package script list is incomplete'
grep -F 'cp "$KERNELSU_ROOT/$script" "$STAGING/$script"' "$PACKAGER" >/dev/null || fail 'package omits fixed script copy'
grep -F 'cp "$GENERATED_PROFILE/target-profile.sh"' "$PACKAGER" >/dev/null || fail 'package omits generated target profile'
grep -F 'crate::target::BRIDGE_CLASSPATH' "$BRIDGE_SOURCE" >/dev/null || fail 'bridge path is not profile-generated'
grep -F 'crate::target::FSPROBE_PATH' "$MATERIALIZER_COMMAND" >/dev/null || fail 'fsprobe path is not profile-generated'

grep -F 'PACKAGE=$UCLONE_TARGET_PACKAGE' "$EMERGENCY" >/dev/null || fail 'emergency containment does not load the compiled target'
grep -F 'launch_builtin_containment' "$POST_FS" >/dev/null || fail 'post-fs lacks self-contained generic containment'
grep -F 'rescue-retired-packages.sh' "$POST_FS" >/dev/null || fail 'post-fs omits typed rescue fallback discovery'
grep -F -- '--active-control' "$POST_FS" >/dev/null || fail 'post-fs omits active-control rescue scan'
grep -F 'JOURNAL_PACKAGES=$SCRIPT_DIR/journal-packages.sh' "$KERNELSU_ROOT/rescue-retired-packages.sh" >/dev/null || fail 'typed rescue helper omits journal verification'
if grep -F 'JOURNAL_PACKAGES=' "$POST_FS" "$EMERGENCY" >/dev/null; then
    fail 'post-fs or emergency bypasses the unified active-control scan'
fi
grep -F 'journal/transactions' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'paired upgrade accepts unfinished transactions'
grep -F -- '--active-control' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'paired upgrade omits the unified active-control scan'
grep -F 'rescue-retired-packages.sh' "$EMERGENCY" >/dev/null || fail 'emergency containment omits typed rescue state'
grep -F -- '--active-control' "$EMERGENCY" >/dev/null || fail 'emergency containment omits active-control rescue scan'
grep -F 'package_is_disabled "$package" && package_is_quiesced "$package"' "$EMERGENCY" >/dev/null || fail 'emergency containment does not prove both safety conditions'
if grep -E 'com[.](uclone[.]slotprobe|asksky[.]fitness)' "$EMERGENCY" >/dev/null; then
    fail 'emergency containment retains a stale literal package'
fi

for script in "$SERVICE" "$BOOT" "$STARTUP"; do
    if grep -E 'package (disable-user|enable)|am force-stop|/system/bin/am' "$script" >/dev/null; then
        fail "$(basename "$script") bypasses the exact gate lease"
    fi
done

for script in post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh boot-completed.sh rescue.sh; do
    grep -F 'case "$owner" in 0:*)' "$KERNELSU_ROOT/$script" >/dev/null || \
        fail "$script rejects a root-owned Android system binary with a non-root group"
    if grep -F '[ "$owner" = "0:0" ]' "$KERNELSU_ROOT/$script" >/dev/null; then
        fail "$script still requires the non-portable root:root binary owner"
    fi
done

if [ "$PROFILE" = generic ]; then
    HANG_FIXTURE=$SCRATCH/hanging-post-fs
    HANG_BIN=$HANG_FIXTURE/bin
    HANG_STATE=$HANG_FIXTURE/state
    HANG_RUNTIME=$HANG_FIXTURE/runtime
    mkdir -p "$HANG_BIN" "$HANG_STATE" \
        "$HANG_RUNTIME/enrollment/packages/com.example.hang"
    sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$HANG_RUNTIME'|" \
        "$SCRATCH/target-profile-generic/target-profile.sh" \
        >"$HANG_FIXTURE/target-profile.sh"
    chmod 444 "$HANG_FIXTURE/target-profile.sh"

    cat >"$HANG_BIN/toybox" <<'EOF'
#!/bin/sh
case "${1:-}" in
    stat)
        case "${2:-}:${3:-}" in
            -c:%u:%g) printf '%s\n' '0:0' ;;
            -c:%u:%a) printf '%s\n' '0:444' ;;
            -c:%u) printf '%s\n' '0' ;;
            -c:%a) printf '%s\n' '755' ;;
            *) exit 1 ;;
        esac ;;
    grep) shift; exec /usr/bin/grep "$@" ;;
    sed) shift; exec /usr/bin/sed "$@" ;;
    sort) shift; exec /usr/bin/sort "$@" ;;
    mkdir|chmod|mv|rm)
        tool=$1
        shift
        exec "/bin/$tool" "$@" ;;
    sleep) shift; exec /bin/sleep "$@" ;;
    ps) exit 0 ;;
    timeout)
        shift
        if [ "${1:-}" = -s ]; then shift 2; fi
        seconds=${1:-}
        [ -n "$seconds" ] || exit 2
        shift
        "$@" &
        child=$!
        ticks=$((seconds * 100))
        tick=0
        while [ "$tick" -lt "$ticks" ]; do
            child_state=$(/bin/ps -p "$child" -o state= 2>/dev/null) || break
            case "$child_state" in *Z*) break ;; esac
            /bin/sleep 0.01
            tick=$((tick + 1))
        done
        kill -9 "$child" 2>/dev/null || :
        status=0
        wait "$child" 2>/dev/null || status=$?
        exit "$status" ;;
    *) exit 1 ;;
esac
EOF
    cat >"$HANG_BIN/cmd" <<EOF
#!/bin/sh
if [ "\${1:-}:\${2:-}" = package:disable-user ]; then
    if [ "\$(cat '$HANG_STATE/hang-command')" = cmd ]; then
        printf '%s\n' "\$\$" >>'$HANG_STATE/hung-pids'
        : >'$HANG_STATE/cmd-entered'
        exec /bin/sleep 300
    fi
    : >'$HANG_STATE/disabled'
    exit 0
fi
if [ "\${1:-}:\${2:-}" = package:list ]; then
    [ ! -e '$HANG_STATE/disabled' ] || printf '%s\n' 'package:com.example.hang'
    exit 0
fi
exit 1
EOF
    cat >"$HANG_BIN/am" <<EOF
#!/bin/sh
if [ "\${1:-}" = force-stop ]; then
    if [ "\$(cat '$HANG_STATE/hang-command')" = am ]; then
        printf '%s\n' "\$\$" >>'$HANG_STATE/hung-pids'
        : >'$HANG_STATE/am-entered'
        exec /bin/sleep 300
    fi
    exit 0
fi
exit 1
EOF
    chmod 755 "$HANG_BIN/toybox" "$HANG_BIN/cmd" "$HANG_BIN/am"
    sed "s|/system/bin/toybox|$HANG_BIN/toybox|g" \
        "$KERNELSU_ROOT/profile-loader.sh" >"$HANG_FIXTURE/profile-loader.sh"
    cat >"$HANG_FIXTURE/journal-packages.sh" <<'EOF'
#!/bin/sh
printf '%s\n' com.example.hang
EOF
    sed -e '1s|.*|#!/bin/sh|' \
        -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$HANG_BIN/toybox'|" \
        -e "s|^RUNTIME_ROOT=.*$|RUNTIME_ROOT='$HANG_RUNTIME'|" \
        -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$HANG_RUNTIME'|" \
        "$KERNELSU_ROOT/rescue-retired-packages.sh" >"$HANG_FIXTURE/rescue-retired-packages.sh"
    cat >"$HANG_FIXTURE/post-fs-setup.sh" <<'EOF'
#!/bin/sh
exit 0
EOF
    cat >"$HANG_FIXTURE/startup-gate.sh" <<'EOF'
#!/bin/sh
exit 0
EOF
    chmod 755 "$HANG_FIXTURE/profile-loader.sh" \
        "$HANG_FIXTURE/journal-packages.sh" "$HANG_FIXTURE/rescue-retired-packages.sh" \
        "$HANG_FIXTURE/post-fs-setup.sh" \
        "$HANG_FIXTURE/startup-gate.sh"
    sed -e '1s|.*|#!/bin/sh|' \
        -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$HANG_BIN/toybox'|" \
        -e "s|^CMD_BIN=.*$|CMD_BIN='$HANG_BIN/cmd'|" \
        -e "s|^AM_BIN=.*$|AM_BIN='$HANG_BIN/am'|" \
        -e "s|^RUNTIME_BIN=.*$|RUNTIME_BIN='$HANG_BIN/missing-ucloned'|" \
        -e "s|^RUNTIME_ROOT=.*$|RUNTIME_ROOT='$HANG_RUNTIME'|" \
        -e 's|^COMMAND_TIMEOUT_SECONDS=.*$|COMMAND_TIMEOUT_SECONDS=1|' \
        -e 's|^HELPER_TIMEOUT_SECONDS=.*$|HELPER_TIMEOUT_SECONDS=1|' \
        -e 's|^RUNTIME_TIMEOUT_SECONDS=.*$|RUNTIME_TIMEOUT_SECONDS=1|' \
        "$EMERGENCY" >"$HANG_FIXTURE/emergency-containment.sh"
    sed -e '1s|.*|#!/bin/sh|' \
        -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$HANG_BIN/toybox'|" \
        -e "s|^CMD_BIN=.*$|CMD_BIN='$HANG_BIN/cmd'|" \
        -e "s|^AM_BIN=.*$|AM_BIN='$HANG_BIN/am'|" \
        -e "s|^RUNTIME_ROOT=.*$|RUNTIME_ROOT='$HANG_RUNTIME'|" \
        -e 's|^POST_FS_HOOK_BUDGET_SECONDS=.*$|POST_FS_HOOK_BUDGET_SECONDS=5|' \
        -e 's|^POST_FS_SETUP_TIMEOUT_SECONDS=.*$|POST_FS_SETUP_TIMEOUT_SECONDS=1|' \
        -e 's|^POST_FS_CONTAINMENT_TIMEOUT_SECONDS=.*$|POST_FS_CONTAINMENT_TIMEOUT_SECONDS=3|' \
        -e 's|^COMMAND_TIMEOUT_SECONDS=.*$|COMMAND_TIMEOUT_SECONDS=1|' \
        -e 's|^HELPER_TIMEOUT_SECONDS=.*$|HELPER_TIMEOUT_SECONDS=1|' \
        "$POST_FS" >"$HANG_FIXTURE/post-fs-data.sh"
    chmod 755 "$HANG_FIXTURE/emergency-containment.sh" \
        "$HANG_FIXTURE/post-fs-data.sh"

    for hanging_command in cmd am; do
        rm -f "$HANG_STATE/disabled" "$HANG_STATE/cmd-entered" \
            "$HANG_STATE/am-entered" "$HANG_STATE/hung-pids" \
            "$HANG_RUNTIME/state/emergency-containment.request"
        printf '%s\n' "$hanging_command" >"$HANG_STATE/hang-command"
        started=$SECONDS
        /bin/sh "$HANG_FIXTURE/post-fs-data.sh"
        elapsed=$((SECONDS - started))
        [ "$elapsed" -le 5 ] || fail "post-fs exceeded fixture budget with hanging $hanging_command (${elapsed}s)"
        [ -e "$HANG_STATE/$hanging_command-entered" ] || \
            fail "hanging $hanging_command path was not exercised"
        request=$HANG_RUNTIME/state/emergency-containment.request
        [ -f "$request" ] && [ ! -L "$request" ] || \
            fail "hanging $hanging_command did not retain a durable containment request"
        grep -F -x 'state=pending' "$request" >/dev/null || \
            fail "hanging $hanging_command request is not pending"
        watcher_pid=$(sed -n 's/^watcher_pid=//p' "$request")
        [ -n "$watcher_pid" ] && kill -0 "$watcher_pid" 2>/dev/null || \
            fail "hanging $hanging_command did not retain a live containment watcher"
        kill "$watcher_pid" 2>/dev/null || true
        if [ -f "$HANG_STATE/hung-pids" ]; then
            while IFS= read -r hung_pid; do
                kill -9 "$hung_pid" 2>/dev/null || true
            done <"$HANG_STATE/hung-pids"
        fi
        printf 'bounded_post_fs=proved hanging=%s elapsed_seconds=%s request=%s watcher=retained\n' \
            "$hanging_command" "$elapsed" "$request"
    done
fi

printf 'KernelSU boot-flow host contract passed profile=%s package=%s.\n' \
    "$PROFILE" "$UCLONE_TARGET_PACKAGE"
