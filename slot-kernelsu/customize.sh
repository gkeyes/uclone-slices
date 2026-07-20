#!/system/bin/sh

set_perm_recursive "$MODPATH" 0 0 0700 0600
set_perm "$MODPATH/module.prop" 0 0 0644
set_perm "$MODPATH/disable" 0 0 0644
set_perm "$MODPATH/skip_mount" 0 0 0644
set_perm "$MODPATH/target-profile.sh" 0 0 0444
set_perm "$MODPATH/runtime/slot-bridge.apk" 0 0 0600

for executable in \
    "$MODPATH/bin/ucloned" "$MODPATH/bin/slotctl" \
    "$MODPATH/runtime/slot-fsprobe" "$MODPATH/post-fs-data.sh" \
    "$MODPATH/emergency-containment.sh" "$MODPATH/startup-gate.sh" \
    "$MODPATH/service.sh" "$MODPATH/boot-completed.sh" \
    "$MODPATH/boot-state.sh" "$MODPATH/rescue.sh"
do
    set_perm "$executable" 0 0 0700
done
