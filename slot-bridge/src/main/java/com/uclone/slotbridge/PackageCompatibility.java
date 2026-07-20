package com.uclone.slotbridge;

import android.content.pm.ApplicationInfo;
import android.content.pm.ComponentInfo;
import android.content.pm.PackageInfo;

final class PackageCompatibility {
    private PackageCompatibility() {}

    static boolean isSystemApp(ApplicationInfo info) {
        int mask = ApplicationInfo.FLAG_SYSTEM | ApplicationInfo.FLAG_UPDATED_SYSTEM_APP;
        return (info.flags & mask) != 0;
    }

    static boolean hasDirectBootAwareComponent(PackageInfo info) {
        return anyDirectBootAware(info.activities)
                || anyDirectBootAware(info.receivers)
                || anyDirectBootAware(info.services)
                || anyDirectBootAware(info.providers);
    }

    private static boolean anyDirectBootAware(ComponentInfo[] components) {
        if (components == null) {
            return false;
        }
        for (ComponentInfo component : components) {
            if (component == null || component.directBootAware) {
                return true;
            }
        }
        return false;
    }
}
