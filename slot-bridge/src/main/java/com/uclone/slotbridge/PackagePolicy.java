package com.uclone.slotbridge;

import android.annotation.SuppressLint;

final class PackagePolicy {
    static final String ALLOWED_PACKAGE = TargetProfile.PACKAGE;
    static final int ALLOWED_USER_ID = TargetProfile.USER_ID;

    private PackagePolicy() {}

    static boolean isAllowed(String packageName) {
        return ALLOWED_PACKAGE.equals(packageName);
    }

    @SuppressLint("SdCardPath")
    static PackagePaths pathsFor(String packageName) throws BridgeFailure {
        if (!isAllowed(packageName)) {
            throw new BridgeFailure("package", ErrorCode.PACKAGE_NOT_ALLOWED);
        }
        return new PackagePaths(TargetProfile.TARGET_CE, TargetProfile.TARGET_DE);
    }

    static boolean isSafeCodePath(String value) {
        return value != null
                && value.length() <= 4096
                && value.startsWith("/data/app/")
                && value.endsWith(".apk")
                && !value.contains("..")
                && !value.contains("//")
                && !value.contains("\\")
                && value.indexOf('\0') < 0
                && value.indexOf('\n') < 0
                && value.indexOf('\r') < 0;
    }
}
