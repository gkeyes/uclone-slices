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
trap 'chmod -R u+w "$SCRATCH" 2>/dev/null || true; rm -rf "$SCRATCH"' EXIT HUP INT TERM
"$REPO_ROOT/tools/render-target-profile.sh" "$PROFILE" "$SCRATCH" >/dev/null
. "$SCRATCH/target-profile-$PROFILE/target-profile.sh"

fail() {
    printf 'boot-flow test failed: %s\n' "$1" >&2
    exit 1
}

for script in customize.sh post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh startup-gate.sh service.sh boot-completed.sh boot-state.sh profile-loader.sh rescue.sh; do
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
SERVICE="$KERNELSU_ROOT/service.sh"
BOOT="$KERNELSU_ROOT/boot-completed.sh"
STARTUP="$KERNELSU_ROOT/startup-gate.sh"
EMERGENCY="$KERNELSU_ROOT/emergency-containment.sh"
PACKAGER="$REPO_ROOT/tools/package-kernelsu-preview.sh"
BRIDGE_SOURCE="$REPO_ROOT/slot-runtime/src/bridge/mod.rs"
MATERIALIZER_COMMAND="$REPO_ROOT/slot-runtime/src/android/materializer/command.rs"
PACKAGE_ZIP="${UCLONE_KERNELSU_PACKAGE:-}"

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
grep -F '"$EMERGENCY_CONTAINMENT" --once' "$POST_FS" >/dev/null || fail 'post-fs does not synchronously prove emergency containment'
grep -F '("$STARTUP_GATE" >/dev/null 2>&1) &' "$POST_FS" >/dev/null || fail 'post-fs does not launch startup reconciliation without blocking boot'
grep -F 'emergency-containment.sh' "$POST_FS" >/dev/null || fail 'post-fs lacks the emergency containment fallback'
grep -F '"$MODPATH/post-fs-setup.sh"' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'customize does not restore post-fs setup executable mode'
grep -F '"$MODPATH/journal-packages.sh"' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'customize does not restore journal helper executable mode'
grep -F 'startup-gate.ready' "$SERVICE" >/dev/null || fail 'service does not require boot-scoped gate proof'
grep -F 'emergency-containment.sh' "$SERVICE" >/dev/null || fail 'service lacks the emergency containment fallback'
grep -F -- '--startup-gate' "$STARTUP" >/dev/null || fail 'startup worker does not use the exact-lease Rust path'
grep -F '"$EMERGENCY_CONTAINMENT" --once' "$STARTUP" >/dev/null || fail 'startup failure does not synchronously contain known packages'
grep -F 'emergency-containment.sh' "$STARTUP" >/dev/null || fail 'startup worker lacks the emergency containment fallback'
grep -F 'BOOT_RECONCILER' "$SERVICE" >/dev/null || fail 'service lacks redundant delayed-unlock worker'
grep -F 'case "$marker_action:$classification" in' "$BOOT" >/dev/null || fail 'reconcile worker does not apply marker policy'
grep -F 'retain:locked)' "$BOOT" >/dev/null || fail 'reconcile worker lacks locked retry branch'
grep -F 'for script in customize.sh post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh startup-gate.sh service.sh boot-completed.sh boot-state.sh profile-loader.sh rescue.sh' "$PACKAGER" >/dev/null || fail 'package script list is incomplete'
grep -F 'cp "$KERNELSU_ROOT/$script" "$STAGING/$script"' "$PACKAGER" >/dev/null || fail 'package omits fixed script copy'
grep -F 'cp "$GENERATED_PROFILE/target-profile.sh"' "$PACKAGER" >/dev/null || fail 'package omits generated target profile'
grep -F 'crate::target::BRIDGE_CLASSPATH' "$BRIDGE_SOURCE" >/dev/null || fail 'bridge path is not profile-generated'
grep -F 'crate::target::FSPROBE_PATH' "$MATERIALIZER_COMMAND" >/dev/null || fail 'fsprobe path is not profile-generated'

grep -F 'PACKAGE=$UCLONE_TARGET_PACKAGE' "$EMERGENCY" >/dev/null || fail 'emergency containment does not load the compiled target'
grep -F 'launch_builtin_containment' "$POST_FS" >/dev/null || fail 'post-fs lacks self-contained generic containment'
grep -F 'compatibility-policy/packages' "$POST_FS" >/dev/null || fail 'post-fs omits compatibility-policy orphan discovery'
grep -F 'slot-metadata/packages' "$POST_FS" >/dev/null || fail 'post-fs omits slot-metadata orphan discovery'
grep -F 'journal/transactions' "$POST_FS" >/dev/null || fail 'post-fs omits transaction-only orphan detection'
grep -F 'journal/transactions' "$EMERGENCY" >/dev/null || fail 'emergency containment omits transaction-only orphan detection'
grep -F 'journal/transactions' "$KERNELSU_ROOT/customize.sh" >/dev/null || fail 'paired upgrade accepts unfinished transactions'
grep -F 'enrollment-attempts/attempts/$PACKAGE' "$EMERGENCY" >/dev/null || fail 'emergency containment checks the wrong enrollment-attempt path'
grep -F 'rescue-journal/packages/$PACKAGE' "$EMERGENCY" >/dev/null || fail 'emergency containment omits rescue journal state'
grep -F 'package_is_disabled "$package" && package_is_quiesced "$package"' "$EMERGENCY" >/dev/null || fail 'emergency containment does not prove both safety conditions'
if grep -E 'com[.](uclone[.]slotprobe|asksky[.]fitness)' "$EMERGENCY" >/dev/null; then
    fail 'emergency containment retains a stale literal package'
fi

for script in "$SERVICE" "$BOOT" "$STARTUP"; do
    if grep -E 'package (disable-user|enable)|am force-stop|/system/bin/am' "$script" >/dev/null; then
        fail "$(basename "$script") bypasses the exact gate lease"
    fi
done

for script in post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh startup-gate.sh service.sh boot-completed.sh rescue.sh; do
    grep -F 'case "$owner" in 0:*)' "$KERNELSU_ROOT/$script" >/dev/null || \
        fail "$script rejects a root-owned Android system binary with a non-root group"
    if grep -F '[ "$owner" = "0:0" ]' "$KERNELSU_ROOT/$script" >/dev/null; then
        fail "$script still requires the non-portable root:root binary owner"
    fi
done

printf 'KernelSU boot-flow host contract passed profile=%s package=%s.\n' \
    "$PROFILE" "$UCLONE_TARGET_PACKAGE"
