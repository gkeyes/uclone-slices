package com.uclone.slotbridge;

import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.content.pm.SigningInfo;
import java.util.ArrayList;
import java.util.List;

final class PackageReader {
    private static final long PACKAGE_INFO_FLAGS =
            PackageManager.GET_SIGNING_CERTIFICATES
                    | PackageManager.GET_ACTIVITIES
                    | PackageManager.GET_SERVICES
                    | PackageManager.GET_RECEIVERS
                    | PackageManager.GET_PROVIDERS;
    private static final Class<?>[] PACKAGE_INFO_TYPES = {
        String.class, long.class, int.class
    };
    private static final Class<?>[] PACKAGE_USER_TYPES = {String.class, int.class};
    private static final int MAX_INSTALL_SESSIONS = 1024;

    private final Object packageManager;
    private final String packageName;

    PackageReader(Object packageManager, String packageName) throws BridgeFailure {
        if (!PackagePolicy.isAllowed(packageName)) {
            throw new BridgeFailure("package", ErrorCode.PACKAGE_NOT_ALLOWED);
        }
        this.packageManager = packageManager;
        this.packageName = packageName;
    }

    PackageSnapshot read() throws BridgeFailure {
        flushRestrictions("package");
        PackageManagerInodes packageManagerInodes =
                PackageRestrictionsParser.parse(FixedRestrictionsFile.read(), packageName);
        PackageInfo info = packageInfo(PACKAGE_INFO_FLAGS, "package");
        ApplicationInfo applicationInfo = info.applicationInfo;
        if (applicationInfo == null) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        int uid = packageUid("package");
        if (uid < 10_000 || applicationInfo.uid != uid) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        PackagePaths paths = PackagePolicy.pathsFor(packageName);
        validatePaths(applicationInfo, paths);
        long versionCode = info.getLongVersionCode();
        if (versionCode <= 0 || !PackagePolicy.isSafeCodePath(applicationInfo.sourceDir)) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        validateVersionName(info.versionName);
        return new PackageSnapshot(
                packageName,
                uid,
                signingIdentity(info.signingInfo),
                versionCode,
                info.versionName,
                applicationInfo.sourceDir,
                paths,
                packageManagerInodes,
                enabledState("package"),
                suspended("package"),
                hasPendingInstall(),
                PackageCompatibility.isSystemApp(applicationInfo),
                info.sharedUserId != null,
                PackageCompatibility.hasDirectBootAwareComponent(info));
    }

    GateSnapshot readGate() throws BridgeFailure {
        return new GateSnapshot(enabledState("gate"), suspended("gate"));
    }

    void requireInstalled(String requestId) throws BridgeFailure {
        packageInfo(0, requestId);
    }

    String packageName() {
        return packageName;
    }

    void flushRestrictions(String requestId) throws BridgeFailure {
        BinderInvoke.call(
                packageManager,
                requestId,
                remoteFailure(requestId),
                "flushPackageRestrictionsAsUser",
                new Class<?>[] {int.class},
                PackagePolicy.ALLOWED_USER_ID);
    }

    EnabledState enabledState(String requestId) throws BridgeFailure {
        Object value = BinderInvoke.call(
                packageManager,
                requestId,
                remoteFailure(requestId),
                "getApplicationEnabledSetting",
                PACKAGE_USER_TYPES,
                packageName,
                PackagePolicy.ALLOWED_USER_ID);
        if (!(value instanceof Integer)) {
            throw new BridgeFailure(requestId, ErrorCode.INVALID_RESPONSE);
        }
        return EnabledState.fromPlatformValue((Integer) value, requestId);
    }

    boolean suspended(String requestId) throws BridgeFailure {
        Object value = BinderInvoke.call(
                packageManager,
                requestId,
                remoteFailure(requestId),
                "isPackageSuspendedForUser",
                PACKAGE_USER_TYPES,
                packageName,
                PackagePolicy.ALLOWED_USER_ID);
        if (!(value instanceof Boolean)) {
            throw new BridgeFailure(requestId, ErrorCode.INVALID_RESPONSE);
        }
        return (Boolean) value;
    }

    private PackageInfo packageInfo(long flags, String requestId) throws BridgeFailure {
        Object value = BinderInvoke.call(
                packageManager,
                requestId,
                remoteFailure(requestId),
                "getPackageInfo",
                PACKAGE_INFO_TYPES,
                packageName,
                flags,
                PackagePolicy.ALLOWED_USER_ID);
        if (value == null) {
            throw new BridgeFailure(requestId, ErrorCode.PACKAGE_NOT_FOUND);
        }
        if (!(value instanceof PackageInfo)) {
            throw new BridgeFailure(requestId, ErrorCode.INVALID_RESPONSE);
        }
        return (PackageInfo) value;
    }

    private int packageUid(String requestId) throws BridgeFailure {
        Object value = BinderInvoke.call(
                packageManager,
                requestId,
                remoteFailure(requestId),
                "getPackageUid",
                PACKAGE_INFO_TYPES,
                packageName,
                0L,
                PackagePolicy.ALLOWED_USER_ID);
        if (!(value instanceof Integer)) {
            throw new BridgeFailure(requestId, ErrorCode.INVALID_RESPONSE);
        }
        return (Integer) value;
    }

    private boolean hasPendingInstall() throws BridgeFailure {
        Object installer = BinderInvoke.call(
                packageManager,
                "package",
                ErrorCode.INTERNAL,
                "getPackageInstaller",
                new Class<?>[0]);
        Object slice = BinderInvoke.call(
                installer,
                "package",
                ErrorCode.INTERNAL,
                "getAllSessions",
                new Class<?>[] {int.class},
                PackagePolicy.ALLOWED_USER_ID);
        Object rawList = BinderInvoke.call(
                slice,
                "package",
                ErrorCode.INTERNAL,
                "getList",
                new Class<?>[0]);
        if (!(rawList instanceof List<?>)) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        List<?> sessions = (List<?>) rawList;
        if (sessions.size() > MAX_INSTALL_SESSIONS) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        for (Object session : sessions) {
            if (!(session instanceof PackageInstaller.SessionInfo)) {
                throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
            }
            PackageInstaller.SessionInfo info = (PackageInstaller.SessionInfo) session;
            if (packageName.equals(info.getAppPackageName())) {
                return true;
            }
        }
        return false;
    }

    private static String signingIdentity(SigningInfo signingInfo) throws BridgeFailure {
        if (signingInfo == null) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        Signature[] signers = signingInfo.getApkContentsSigners();
        if (signers == null || signers.length == 0) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        List<byte[]> certificates = new ArrayList<>(signers.length);
        for (Signature signer : signers) {
            if (signer == null) {
                throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
            }
            certificates.add(signer.toByteArray());
        }
        return SigningIdentity.aggregate(certificates);
    }

    private static void validatePaths(ApplicationInfo info, PackagePaths expected)
            throws BridgeFailure {
        if (!expected.ceDataPath().equals(info.dataDir)
                || !expected.deDataPath().equals(info.deviceProtectedDataDir)) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
    }

    private static void validateVersionName(String value) throws BridgeFailure {
        if (value == null) {
            return;
        }
        if (value.length() > 128) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            if (Character.isISOControl(character)) {
                throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
            }
        }
    }

    private static ErrorCode remoteFailure(String requestId) {
        return "package".equals(requestId) ? ErrorCode.INTERNAL : ErrorCode.COMMAND_FAILED;
    }
}
