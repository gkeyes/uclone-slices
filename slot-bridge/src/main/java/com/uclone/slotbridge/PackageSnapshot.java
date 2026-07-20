package com.uclone.slotbridge;

final class PackageSnapshot {
    private final String packageName;
    private final int uid;
    private final String signatureSha256;
    private final long versionCode;
    private final String versionName;
    private final String codePath;
    private final PackagePaths dataPaths;
    private final PackageManagerInodes packageManagerInodes;
    private final EnabledState enabledState;
    private final boolean suspended;
    private final boolean pendingInstall;
    private final boolean systemApp;
    private final boolean sharedUid;
    private final boolean directBootAware;

    PackageSnapshot(
            String packageName,
            int uid,
            String signatureSha256,
            long versionCode,
            String versionName,
            String codePath,
            PackagePaths dataPaths,
            PackageManagerInodes packageManagerInodes,
            EnabledState enabledState,
            boolean suspended,
            boolean pendingInstall,
            boolean systemApp,
            boolean sharedUid,
            boolean directBootAware) {
        this.packageName = packageName;
        this.uid = uid;
        this.signatureSha256 = signatureSha256;
        this.versionCode = versionCode;
        this.versionName = versionName;
        this.codePath = codePath;
        this.dataPaths = dataPaths;
        this.packageManagerInodes = packageManagerInodes;
        this.enabledState = enabledState;
        this.suspended = suspended;
        this.pendingInstall = pendingInstall;
        this.systemApp = systemApp;
        this.sharedUid = sharedUid;
        this.directBootAware = directBootAware;
    }

    String packageName() {
        return packageName;
    }

    int uid() {
        return uid;
    }

    String signatureSha256() {
        return signatureSha256;
    }

    long versionCode() {
        return versionCode;
    }

    String versionName() {
        return versionName;
    }

    String codePath() {
        return codePath;
    }

    PackagePaths dataPaths() {
        return dataPaths;
    }

    PackageManagerInodes packageManagerInodes() {
        return packageManagerInodes;
    }

    EnabledState enabledState() {
        return enabledState;
    }

    boolean suspended() {
        return suspended;
    }

    boolean pendingInstall() {
        return pendingInstall;
    }

    boolean systemApp() {
        return systemApp;
    }

    boolean sharedUid() {
        return sharedUid;
    }

    boolean directBootAware() {
        return directBootAware;
    }
}
