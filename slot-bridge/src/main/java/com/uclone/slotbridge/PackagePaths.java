package com.uclone.slotbridge;

final class PackagePaths {
    private final String ceDataPath;
    private final String deDataPath;

    PackagePaths(String ceDataPath, String deDataPath) {
        this.ceDataPath = ceDataPath;
        this.deDataPath = deDataPath;
    }

    String ceDataPath() {
        return ceDataPath;
    }

    String deDataPath() {
        return deDataPath;
    }
}
