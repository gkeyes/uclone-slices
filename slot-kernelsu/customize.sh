#!/system/bin/sh

RUNTIME_ROOT=/data/adb/uclone-slices-preview

root_has_artifact() {
    root="$1"
    [ -d "$root" ] && [ ! -L "$root" ] || return 1
    for entry in "$root"/*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        return 0
    done
    return 1
}

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
    if root_has_artifact "$root"; then
        ui_print "UClone Slots: active Preview metadata exists. Rescue every App to Base before this paired upgrade."
        abort "UClone Slots paired upgrade refused"
    fi
done
for path in "$RUNTIME_ROOT"/state/*.gate "$RUNTIME_ROOT"/state/.*.gate*; do
    [ -e "$path" ] || [ -L "$path" ] || continue
    ui_print "UClone Slots: an active Gate lease exists. Complete Base rescue before upgrade."
    abort "UClone Slots paired upgrade refused"
done
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
    "$MODPATH/boot-state.sh" "$MODPATH/profile-loader.sh" "$MODPATH/rescue.sh"
do
    set_perm "$executable" 0 0 0700
done
