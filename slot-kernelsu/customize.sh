#!/system/bin/sh

RUNTIME_ROOT=/data/adb/uclone-slices-preview
INSTALLED_MODULE=/data/adb/modules/uclone-slices-preview
TOYBOX_BIN=/system/bin/toybox
SYSTEM_SHELL=/system/bin/sh
UPGRADE_REFUSAL=
UPGRADE_APP_COUNT=0
UPGRADE_PROOF_USED=0

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

management_metadata_present() {
    for root in \
        "$RUNTIME_ROOT/enrollment/packages" \
        "$RUNTIME_ROOT/compatibility-policy/packages" \
        "$RUNTIME_ROOT/catalog/packages" \
        "$RUNTIME_ROOT/registry/packages" \
        "$RUNTIME_ROOT/package-state/packages" \
        "$RUNTIME_ROOT/slot-metadata/packages" \
        "$RUNTIME_ROOT/enrollment-attempts/attempts" \
        "$RUNTIME_ROOT/rescue-journal/packages" \
        "$RUNTIME_ROOT/journal/transactions"
    do
        root_has_artifact "$root" && return 0
    done
    return 1
}

active_gate_present() {
    state_root=$RUNTIME_ROOT/state
    if [ -L "$state_root" ] || { [ -e "$state_root" ] && [ ! -d "$state_root" ]; }; then
        return 0
    fi
    [ -d "$state_root" ] || return 1
    for path in "$state_root"/* "$state_root"/.[!.]* "$state_root"/..?*; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        [ -f "$path" ] && [ ! -L "$path" ] || return 0
        name=${path##*/}
        case "$name" in
            .*.gate.retired) ;;
            *) return 0 ;;
        esac
    done
    return 1
}

safe_installed_slotctl() {
    binary=$INSTALLED_MODULE/bin/slotctl
    [ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] || return 1
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u' "$binary" 2>/dev/null)" || return 1
    [ "$owner" = 0 ] || return 1
    mode="$($TOYBOX_BIN stat -c '%a' "$binary" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

safe_upgrade_proof() {
    binary=$MODPATH/prepare-upgrade.sh
    [ -f "$binary" ] && [ ! -L "$binary" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u' "$binary" 2>/dev/null)" || return 1
    [ "$owner" = 0 ] || return 1
    mode="$($TOYBOX_BIN stat -c '%a' "$binary" 2>/dev/null)" || return 1
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}

read_offline_upgrade_proof() {
    safe_upgrade_proof || return 1
    proof_count="$($TOYBOX_BIN timeout -s 9 30 "$SYSTEM_SHELL" "$MODPATH/prepare-upgrade.sh" --verify 2>/dev/null)" || return 1
    case "$proof_count" in *[!0-9]*|'') return 1 ;; esac
    [ "$proof_count" -ge 1 ] && [ "$proof_count" -le 64 ] || return 1
    UPGRADE_APP_COUNT=$proof_count
    UPGRADE_PROOF_USED=1
}

paired_upgrade_ready() {
    if active_gate_present; then
        UPGRADE_REFUSAL='an active App Gate lease still exists'
        return 1
    fi
    if ! safe_installed_slotctl; then
        UPGRADE_REFUSAL='the installed Runtime client is unavailable or unsafe'
        return 1
    fi
    old_slotctl=$INSTALLED_MODULE/bin/slotctl
    apps_frame="$($TOYBOX_BIN timeout -s 9 30 "$old_slotctl" apps 2>/dev/null)" || {
        if read_offline_upgrade_proof; then
            return 0
        fi
        UPGRADE_REFUSAL='the installed Runtime is unavailable and no valid Base upgrade proof exists'
        return 1
    }
    case "$apps_frame" in
        *'"status":"ok"'*'"kind":"managed_apps"'*'"apps":['*) ;;
        *)
            UPGRADE_REFUSAL='the installed Runtime returned an invalid managed-App report'
            return 1
            ;;
    esac
    active_slots="$(printf '%s\n' "$apps_frame" | "$TOYBOX_BIN" grep -o '"active_slot":"[^"]*"' 2>/dev/null)"
    if [ -z "$active_slots" ]; then
        UPGRADE_REFUSAL='management metadata exists without a reportable managed App'
        return 1
    fi
    non_base="$(printf '%s\n' "$active_slots" | "$TOYBOX_BIN" grep -F -v '"active_slot":"base"' 2>/dev/null)"
    if [ -n "$non_base" ]; then
        UPGRADE_REFUSAL='at least one managed App is still using a Preview slot'
        return 1
    fi
    lifecycles="$(printf '%s\n' "$apps_frame" | "$TOYBOX_BIN" grep -o '"lifecycle":"[^"]*"' 2>/dev/null)"
    if [ -z "$lifecycles" ]; then
        UPGRADE_REFUSAL='the managed-App lifecycle report is incomplete'
        return 1
    fi
    non_normal="$(printf '%s\n' "$lifecycles" | "$TOYBOX_BIN" grep -F -v '"lifecycle":"normal"' 2>/dev/null)"
    if [ -n "$non_normal" ]; then
        UPGRADE_REFUSAL='at least one managed App is not in the normal lifecycle state'
        return 1
    fi
    UPGRADE_APP_COUNT="$(printf '%s\n' "$active_slots" | "$TOYBOX_BIN" wc -l | "$TOYBOX_BIN" tr -d '[:space:]')"
    return 0
}

if management_metadata_present; then
    ui_print "UClone Slots: existing managed Apps detected; read-only Base verification before paired upgrade."
    if ! paired_upgrade_ready; then
        ui_print "UClone Slots: $UPGRADE_REFUSAL."
        ui_print "Switch every managed App to Base, run uclone-prepare-upgrade.sh as root, then retry. Existing slots are preserved."
        abort "UClone Slots paired upgrade refused"
    fi
    if [ "$UPGRADE_PROOF_USED" -eq 1 ]; then
        ui_print "UClone Slots: verified $UPGRADE_APP_COUNT managed App(s) from an unchanged one-time Base proof."
    else
        ui_print "UClone Slots: verified $UPGRADE_APP_COUNT managed App(s) on native Base."
    fi
    ui_print "UClone Slots: preserving registrations and slots."
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
    "$MODPATH/journal-packages.sh" "$MODPATH/startup-gate.sh" \
    "$MODPATH/service.sh" "$MODPATH/boot-completed.sh" \
    "$MODPATH/boot-state.sh" "$MODPATH/profile-loader.sh" "$MODPATH/rescue.sh" \
    "$MODPATH/prepare-upgrade.sh"
do
    set_perm "$executable" 0 0 0700
done

if [ "$UPGRADE_PROOF_USED" -eq 1 ]; then
    "$TOYBOX_BIN" timeout -s 9 30 "$SYSTEM_SHELL" "$MODPATH/prepare-upgrade.sh" --consume >/dev/null 2>&1 || \
        abort "UClone Slots could not consume the one-time upgrade proof"
fi
