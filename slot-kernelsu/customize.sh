#!/system/bin/sh

RUNTIME_ROOT=/data/adb/uclone-slices-preview
INSTALLED_MODULE=/data/adb/modules/uclone-slices-preview
TOYBOX_BIN=/system/bin/toybox
SYSTEM_SHELL=/system/bin/sh
UPGRADE_REFUSAL=
UPGRADE_APP_COUNT=0
UPGRADE_PROOF_HELD=0
UPGRADE_STAGING_ACTIVATED=0
ORPHANED_RECOVERY_REINSTALL=0

root_has_artifact() {
    root=$1
    if [ -L "$root" ] || { [ -e "$root" ] && [ ! -d "$root" ]; }; then return 0; fi
    [ -d "$root" ] || return 1
    for entry in "$root"/* "$root"/.[!.]* "$root"/..?*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        return 0
    done
    return 1
}

managed_root_present() {
    for root in \
        "$RUNTIME_ROOT/enrollment/packages" \
        "$RUNTIME_ROOT/compatibility-policy/packages" \
        "$RUNTIME_ROOT/catalog/packages" \
        "$RUNTIME_ROOT/registry/packages" \
        "$RUNTIME_ROOT/package-state/packages" \
        "$RUNTIME_ROOT/slot-metadata/packages" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts" \
        "$RUNTIME_ROOT/rescue-journal/packages" \
        "$RUNTIME_ROOT/journal/transactions" \
        "/data/misc_de/0/uclone-slices-preview/slots"
    do
        root_has_artifact "$root" && return 0
    done
    return 1
}

management_metadata_present() {
    [ -e "$RUNTIME_ROOT" ] || [ -L "$RUNTIME_ROOT" ] || return 1
    managed_root_present || return 1
    helper="$MODPATH/rescue-retired-packages.sh"
    safe_upgrade_file "$helper" || return 0
    active_control="$($TOYBOX_BIN timeout -s 9 8 "$SYSTEM_SHELL" \
        "$helper" --active-control 2>/dev/null)" || return 0
    [ -n "$active_control" ]
}

active_gate_present() {
    state_root=$RUNTIME_ROOT/state
    if [ -L "$state_root" ] || { [ -e "$state_root" ] && [ ! -d "$state_root" ]; }; then return 0; fi
    [ -d "$state_root" ] && [ ! -L "$state_root" ] || return 1
    for path in "$state_root"/* "$state_root"/.[!.]* "$state_root"/..?*; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        [ -f "$path" ] && [ ! -L "$path" ] || return 0
        case "${path##*/}" in .*.gate.retired) ;; *) return 0 ;; esac
    done
    return 1
}

safe_upgrade_file() {
    [ -f "$1" ] && [ ! -L "$1" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u' "$1" 2>/dev/null)" || return 1
    [ "$owner" = 0 ] || return 1
    mode="$($TOYBOX_BIN stat -c '%a' "$1" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

safe_upgrade_directory() {
    [ -d "$1" ] && [ ! -L "$1" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u' "$1" 2>/dev/null)" || return 1
    [ "$owner" = 0 ] || return 1
    mode="$($TOYBOX_BIN stat -c '%a' "$1" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

old_daemon_process_absent() {
    pids="$($TOYBOX_BIN pidof ucloned 2>/dev/null)"
    result=$?
    [ "$result" -eq 1 ] && [ -z "$pids" ]
}

installed_runtime_inactive_for_recovery() {
    if [ ! -e "$INSTALLED_MODULE" ] && [ ! -L "$INSTALLED_MODULE" ]; then
        return 0
    fi
    safe_upgrade_directory "$INSTALLED_MODULE" &&
        safe_upgrade_file "$INSTALLED_MODULE/disable"
}

orphaned_recovery_reinstall_ready() {
    installed_runtime_inactive_for_recovery && old_daemon_process_absent
}

safe_upgrade_tools() {
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] &&
        [ "$($TOYBOX_BIN stat -c '%u' "$TOYBOX_BIN" 2>/dev/null)" = 0 ] &&
        safe_upgrade_file "$MODPATH/prepare-upgrade.sh" &&
        safe_upgrade_file "$MODPATH/upgrade-freeze.sh"
}

run_upgrade_tool() {
    "$TOYBOX_BIN" timeout -s 9 45 "$SYSTEM_SHELL" "$MODPATH/prepare-upgrade.sh" "$1"
}

upgrade_cleanup() {
    if [ "$UPGRADE_STAGING_ACTIVATED" -eq 1 ]; then
        : >"$MODPATH/disable" || return 1
        "$TOYBOX_BIN" chmod 644 "$MODPATH/disable" 2>/dev/null || return 1
        safe_upgrade_file "$MODPATH/disable" || return 1
        UPGRADE_STAGING_ACTIVATED=0
    fi
    if [ "$UPGRADE_PROOF_HELD" -eq 1 ]; then
        run_upgrade_tool --cancel >/dev/null 2>&1 || return 1
        UPGRADE_PROOF_HELD=0
    fi
}

activate_staged_update() {
    safe_upgrade_file "$MODPATH/disable" || return 1
    UPGRADE_STAGING_ACTIVATED=1
    "$TOYBOX_BIN" rm -f "$MODPATH/disable" 2>/dev/null || return 1
    [ ! -e "$MODPATH/disable" ] && [ ! -L "$MODPATH/disable" ]
}

upgrade_interrupted() {
    upgrade_cleanup || true
    trap - 0 HUP INT TERM
    exit 1
}

read_verified_count() {
    proof_count="$(run_upgrade_tool --verify 2>/dev/null)" || return 1
    case "$proof_count" in *[!0-9]*|'') return 1 ;; esac
    [ "$proof_count" -ge 0 ] && [ "$proof_count" -le 64 ] || return 1
    UPGRADE_APP_COUNT=$proof_count
}

paired_upgrade_ready() {
    safe_upgrade_tools || {
        UPGRADE_REFUSAL='the staged upgrade safety tools are missing or unsafe'
        return 1
    }
    if read_verified_count; then
        UPGRADE_PROOF_HELD=1
        trap 'upgrade_cleanup || exit 1' 0
        trap 'upgrade_interrupted' HUP INT TERM
        return 0
    fi
    if ! run_upgrade_tool --prepare >/dev/null 2>&1; then
        run_upgrade_tool --cancel >/dev/null 2>&1 || true
        UPGRADE_REFUSAL='the installed Runtime could not be frozen after proving every managed App on Base'
        return 1
    fi
    UPGRADE_PROOF_HELD=1
    trap 'upgrade_cleanup || exit 1' 0
    trap 'upgrade_interrupted' HUP INT TERM
    if ! read_verified_count; then
        UPGRADE_REFUSAL='the frozen Base proof could not be revalidated'
        return 1
    fi
    return 0
}

if management_metadata_present || active_gate_present; then
    if orphaned_recovery_reinstall_ready; then
        ORPHANED_RECOVERY_REINSTALL=1
        ui_print 'UClone Slots: no active installed Runtime is available and no ucloned process is live.'
        ui_print 'UClone Slots: preserving all existing management state for recovery reinstall.'
        ui_print 'Recovery reinstall keeps the staged module disabled and does not prove Base, retired, or complete.'
    else
        ui_print 'UClone Slots: existing managed Apps detected; freezing the old Runtime for paired upgrade.'
        if ! paired_upgrade_ready; then
            ui_print "UClone Slots: $UPGRADE_REFUSAL."
            ui_print 'Switch every managed App to Base, wait for completion, then retry. Existing slots are preserved.'
            abort 'UClone Slots paired upgrade refused'
        fi
        ui_print "UClone Slots: verified $UPGRADE_APP_COUNT managed App(s) on native Base with the old Runtime stopped."
        ui_print 'UClone Slots: preserving registrations and slots.'
    fi
fi

set_perm_recursive "$MODPATH" 0 0 0700 0600
set_perm "$MODPATH/module.prop" 0 0 0644
set_perm "$MODPATH/disable" 0 0 0644
set_perm "$MODPATH/skip_mount" 0 0 0644
set_perm "$MODPATH/target-profile.sh" 0 0 0444
set_perm "$MODPATH/runtime/slot-bridge.apk" 0 0 0600

for executable in \
    "$MODPATH/bin/ucloned" "$MODPATH/bin/slotctl" \
    "$MODPATH/runtime/slot-fsprobe" "$MODPATH/post-fs-data.sh" \
    "$MODPATH/post-fs-setup.sh" "$MODPATH/emergency-containment.sh" \
    "$MODPATH/journal-packages.sh" "$MODPATH/rescue-retired-packages.sh" \
    "$MODPATH/startup-gate.sh" \
    "$MODPATH/service.sh" "$MODPATH/boot-completed.sh" \
    "$MODPATH/boot-state.sh" "$MODPATH/profile-loader.sh" "$MODPATH/rescue.sh" \
    "$MODPATH/prepare-upgrade.sh" "$MODPATH/upgrade-freeze.sh"
do
    set_perm "$executable" 0 0 0700
done

if [ "$UPGRADE_PROOF_HELD" -eq 1 ]; then
    if ! "$TOYBOX_BIN" timeout -s 9 45 "$SYSTEM_SHELL" "$MODPATH/prepare-upgrade.sh" --stage >/dev/null 2>&1; then
        abort 'UClone Slots could not hand the frozen Runtime to the staged module'
    fi
    activate_staged_update || abort 'UClone Slots could not preserve the active module state for reboot'
    trap '' 0 HUP INT TERM
    UPGRADE_PROOF_HELD=0
    UPGRADE_STAGING_ACTIVATED=0
    trap - 0 HUP INT TERM
    ui_print 'UClone Slots: old Runtime remains safely stopped until reboot activates the paired update.'
    ui_print 'To cancel before reboot, run the staged root-owned prepare-upgrade.sh --cancel, then remove the staged module update.'
fi

if [ "$ORPHANED_RECOVERY_REINSTALL" -eq 1 ]; then
    safe_upgrade_file "$MODPATH/disable" ||
        abort 'UClone Slots recovery reinstall lost its staged disable marker'
    old_daemon_process_absent ||
        abort 'UClone Slots recovery reinstall observed a new daemon and remains disabled'
    ui_print 'UClone Slots: recovery Runtime installed disabled; no active-slot claim was changed.'
fi
