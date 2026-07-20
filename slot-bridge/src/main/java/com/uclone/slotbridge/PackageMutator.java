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

    void setEnabled(EnabledState state) throws BridgeFailure {
        String requestId = "set-enabled";
        reader.requireInstalled(requestId);
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

    void setSuspended(boolean suspended) throws BridgeFailure {
        String requestId = "set-suspended";
        reader.requireInstalled(requestId);
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
