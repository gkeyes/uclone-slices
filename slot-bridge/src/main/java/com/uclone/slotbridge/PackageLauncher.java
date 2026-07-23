package com.uclone.slotbridge;

import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ResolveInfo;
import android.os.Bundle;
import android.os.IBinder;
import java.util.List;

final class PackageLauncher {
    private static final String REQUEST_ID = "launch-package";
    private static final int FIRST_START_SUCCESS = 0;
    private static final int LAST_START_SUCCESS = 99;
    private static final int MAX_LAUNCHER_ENTRIES = 64;

    private final Object packageManager;
    private final Object activityTaskManager;
    private final PackageReader reader;

    PackageLauncher(Object packageManager, Object activityTaskManager, PackageReader reader) {
        this.packageManager = packageManager;
        this.activityTaskManager = activityTaskManager;
        this.reader = reader;
    }

    void launch(ExpectedPackageIdentity expectedIdentity) throws BridgeFailure {
        Intent intent = resolve(Intent.CATEGORY_INFO);
        if (intent == null) {
            intent = resolve(Intent.CATEGORY_LAUNCHER);
        }
        if (intent == null) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.LAUNCH_ENTRY_NOT_FOUND);
        }
        requireExpectedIdentity(expectedIdentity);
        Object result = BinderInvoke.call(
                activityTaskManager,
                REQUEST_ID,
                ErrorCode.COMMAND_FAILED,
                "startActivityAsUser",
                startParameterTypes(),
                null,
                null,
                null,
                intent,
                null,
                null,
                null,
                0,
                0,
                null,
                null,
                PackagePolicy.ALLOWED_USER_ID);
        if (!(result instanceof Integer)
                || (Integer) result < FIRST_START_SUCCESS
                || (Integer) result > LAST_START_SUCCESS) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.COMMAND_FAILED);
        }
    }

    private void requireExpectedIdentity(ExpectedPackageIdentity expected) throws BridgeFailure {
        PackageSnapshot actual;
        try {
            actual = reader.read();
        } catch (BridgeFailure failure) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.IDENTITY_CHANGED);
        }
        if (expected == null) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.INVALID_REQUEST);
        }
        expected.requireMatches(actual, REQUEST_ID);
    }

    private Intent resolve(String category) throws BridgeFailure {
        Intent candidate = new Intent(Intent.ACTION_MAIN)
                .addCategory(category)
                .setPackage(reader.packageName());
        Object slice = BinderInvoke.call(
                packageManager,
                REQUEST_ID,
                ErrorCode.COMMAND_FAILED,
                "queryIntentActivities",
                new Class<?>[] {Intent.class, String.class, long.class, int.class},
                candidate,
                null,
                0L,
                PackagePolicy.ALLOWED_USER_ID);
        if (slice == null) {
            return null;
        }
        Object value = BinderInvoke.call(
                slice,
                REQUEST_ID,
                ErrorCode.COMMAND_FAILED,
                "getList",
                new Class<?>[0]);
        if (!(value instanceof List<?>)) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.INVALID_RESPONSE);
        }
        List<?> entries = (List<?>) value;
        if (entries.isEmpty()) {
            return null;
        }
        if (entries.size() > MAX_LAUNCHER_ENTRIES
                || !(entries.get(0) instanceof ResolveInfo)) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.INVALID_RESPONSE);
        }
        ActivityInfo activity = ((ResolveInfo) entries.get(0)).activityInfo;
        if (activity == null || !reader.packageName().equals(activity.packageName)) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.INVALID_RESPONSE);
        }
        return new Intent(candidate)
                .setPackage(null)
                .setClassName(activity.packageName, activity.name)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
    }

    private static Class<?>[] startParameterTypes() throws BridgeFailure {
        return new Class<?>[] {
            hiddenClass("android.app.IApplicationThread"),
            String.class,
            String.class,
            Intent.class,
            String.class,
            IBinder.class,
            String.class,
            int.class,
            int.class,
            hiddenClass("android.app.ProfilerInfo"),
            Bundle.class,
            int.class,
        };
    }

    private static Class<?> hiddenClass(String name) throws BridgeFailure {
        try {
            return Class.forName(name);
        } catch (ClassNotFoundException | LinkageError failure) {
            throw new BridgeFailure(REQUEST_ID, ErrorCode.COMMAND_FAILED);
        }
    }
}
