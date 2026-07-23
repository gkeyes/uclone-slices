package com.uclone.slotbridge;

import android.annotation.SuppressLint;

final class PackagePolicy {
    static final int ALLOWED_USER_ID = 0;

    private PackagePolicy() {}

    static boolean isAllowed(String packageName) {
        if (packageName == null || packageName.isEmpty() || packageName.length() > 255) {
            return false;
        }
        String[] segments = packageName.split("\\.", -1);
        if (segments.length < 2) {
            return false;
        }
        for (String segment : segments) {
            if (!isValidSegment(segment)) {
                return false;
            }
        }
        return true;
    }

    @SuppressLint("SdCardPath")
    static PackagePaths pathsFor(String packageName) throws BridgeFailure {
        if (!isAllowed(packageName)) {
            throw new BridgeFailure("package", ErrorCode.PACKAGE_NOT_ALLOWED);
        }
        return new PackagePaths(
                "/data/user/0/" + packageName,
                "/data/user_de/0/" + packageName);
    }

    private static boolean isValidSegment(String segment) {
        if (segment.isEmpty() || !isAsciiLetter(segment.charAt(0))) {
            return false;
        }
        for (int index = 1; index < segment.length(); index++) {
            char value = segment.charAt(index);
            if (!isAsciiLetter(value) && !isAsciiDigit(value) && value != '_') {
                return false;
            }
        }
        return true;
    }

    private static boolean isAsciiLetter(char value) {
        return (value >= 'a' && value <= 'z') || (value >= 'A' && value <= 'Z');
    }

    private static boolean isAsciiDigit(char value) {
        return value >= '0' && value <= '9';
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
                && value.indexOf('\t') < 0
                && value.indexOf('\n') < 0
                && value.indexOf('\r') < 0;
    }
}
