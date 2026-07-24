#!/system/bin/sh

ui_print "- Installing UClone Slices V2"
set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/bin/ucloned" 0 0 0755
set_perm "$MODPATH/bin/slotctl" 0 0 0755

