package com.uclone.slotbridge;

import java.util.Map;

final class AbxProcessEnvironment {
    static final int MAX_BOOTCLASSPATH_BYTES = 64 * 1024;

    private static final String BOOTCLASSPATH = "BOOTCLASSPATH";
    private static final String DEX2OATBOOTCLASSPATH = "DEX2OATBOOTCLASSPATH";
    private static final String ANDROID_ROOT = "/system";
    private static final String ANDROID_DATA = "/data";
    private static final String ANDROID_ART_ROOT = "/apex/com.android.art";
    private static final String ANDROID_I18N_ROOT = "/apex/com.android.i18n";
    private static final String ANDROID_TZDATA_ROOT = "/apex/com.android.tzdata";

    private AbxProcessEnvironment() {}

    static void populate(Map<String, String> environment, Map<String, String> source)
            throws BridgeFailure {
        if (environment == null || source == null) {
            throw invalid();
        }
        String bootClasspath = validateClasspath(source.get(BOOTCLASSPATH));
        String dex2oatBootClasspath = validateClasspath(source.get(DEX2OATBOOTCLASSPATH));
        environment.clear();
        environment.put("CLASSPATH", "/system/framework/abx.jar");
        environment.put("ANDROID_ROOT", ANDROID_ROOT);
        environment.put("ANDROID_DATA", ANDROID_DATA);
        environment.put("ANDROID_ART_ROOT", ANDROID_ART_ROOT);
        environment.put("ANDROID_I18N_ROOT", ANDROID_I18N_ROOT);
        environment.put("ANDROID_TZDATA_ROOT", ANDROID_TZDATA_ROOT);
        environment.put("PATH", "/system/bin");
        environment.put(BOOTCLASSPATH, bootClasspath);
        environment.put(DEX2OATBOOTCLASSPATH, dex2oatBootClasspath);
    }

    private static String validateClasspath(String value) throws BridgeFailure {
        if (value == null || value.isEmpty() || value.length() > MAX_BOOTCLASSPATH_BYTES) {
            throw invalid();
        }
        String[] entries = value.split(":", -1);
        for (String entry : entries) {
            if (entry.isEmpty()
                    || !entry.startsWith("/")
                    || entry.contains("..")
                    || entry.contains("//")
                    || !trustedClasspathEntry(entry)) {
                throw invalid();
            }
            for (int index = 0; index < entry.length(); index++) {
                char character = entry.charAt(index);
                if (character > 0x7f
                        || character == '\\'
                        || Character.isISOControl(character)
                        || Character.isWhitespace(character)) {
                    throw invalid();
                }
            }
        }
        return value;
    }

    private static boolean trustedClasspathEntry(String entry) {
        if (entry.startsWith("/system/framework/")
                || entry.startsWith("/system_ext/framework/")) {
            return entry.length() > entry.lastIndexOf('/') + 1;
        }
        if (!entry.startsWith("/apex/")) {
            return false;
        }
        int javalib = entry.indexOf("/javalib/", "/apex/".length());
        return javalib > "/apex/".length()
                && javalib + "/javalib/".length() < entry.length();
    }

    private static BridgeFailure invalid() {
        return new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
    }
}
