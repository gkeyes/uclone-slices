package com.uclone.slotbridge;

import android.annotation.SuppressLint;
import android.os.PersistableBundle;

final class PackageMutator {
    private static final Class<?>[] SET_ENABLED_TYPES = {
        String.class, int.class, int.class, int.class, String.class
    };
    private final Object packageManager;
    private final PackageReader reader;

    PackageMutator(Object packageManager, PackageReader reader) {
        this.packageManager = packageManager;
        this.reader = reader;
    }

    void disableUser() throws BridgeFailure {
        String requestId = "set-enabled";
        reader.requireInstalled(requestId);
        writeEnabled(EnabledState.DISABLED_USER, requestId);
    }

    void restoreEnabled(EnabledState state, ExpectedPackageIdentity expected)
            throws BridgeFailure {
        String requestId = "restore-enabled";
        requireExpected(expected, requestId);
        writeEnabled(state, requestId);
    }

    private void writeEnabled(EnabledState state, String requestId) throws BridgeFailure {
        BinderInvoke.call(
                packageManager,
                requestId,
                ErrorCode.COMMAND_FAILED,
                "setApplicationEnabledSetting",
                SET_ENABLED_TYPES,
                reader.packageName(),
                state.platformValue(),
                MutationPolicy.SYNCHRONOUS_ENABLED_FLAGS,
                PackagePolicy.ALLOWED_USER_ID,
                null);
        if (reader.enabledState(requestId) != state) {
            throw new BridgeFailure(requestId, ErrorCode.COMMAND_FAILED);
        }
    }

    void restoreSuspended(boolean suspended, ExpectedPackageIdentity expected)
            throws BridgeFailure {
        String requestId = "restore-suspended";
        requireExpected(expected, requestId);
        Object value = BinderInvoke.call(
                packageManager,
                requestId,
                ErrorCode.COMMAND_FAILED,
                "setPackagesSuspendedAsUser",
                suspendedParameterTypes(requestId),
                new String[] {reader.packageName()},
                suspended,
                null,
                null,
                null,
                0,
                "android",
                PackagePolicy.ALLOWED_USER_ID,
                PackagePolicy.ALLOWED_USER_ID);
        if (!(value instanceof String[]) || ((String[]) value).length != 0) {
            throw new BridgeFailure(requestId, ErrorCode.COMMAND_FAILED);
        }
        reader.flushRestrictions(requestId);
        if (reader.suspended(requestId) != suspended) {
            throw new BridgeFailure(requestId, ErrorCode.COMMAND_FAILED);
        }
    }

    private void requireExpected(ExpectedPackageIdentity expected, String requestId)
            throws BridgeFailure {
        if (expected == null) {
            throw new BridgeFailure(requestId, ErrorCode.INVALID_REQUEST);
        }
        PackageSnapshot actual;
        try {
            actual = reader.read();
        } catch (BridgeFailure failure) {
            throw new BridgeFailure(requestId, ErrorCode.PACKAGE_STATE_CHANGED);
        }
        expected.requireMatches(actual, requestId);
    }

    @SuppressLint("PrivateApi")
    private static Class<?>[] suspendedParameterTypes(String requestId) throws BridgeFailure {
        try {
            return new Class<?>[] {
                String[].class,
                boolean.class,
                PersistableBundle.class,
                PersistableBundle.class,
                Class.forName("android.content.pm.SuspendDialogInfo"),
                int.class,
                String.class,
                int.class,
                int.class
            };
        } catch (ClassNotFoundException | LinkageError failure) {
            throw new BridgeFailure(requestId, ErrorCode.COMMAND_FAILED);
        }
    }
}
