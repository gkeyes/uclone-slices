package com.uclone.slotbridge;

import android.annotation.SuppressLint;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

final class BinderServices {
    private final Object packageManager;
    private final Object userManager;

    private BinderServices(Object packageManager, Object userManager) {
        this.packageManager = packageManager;
        this.userManager = userManager;
    }

    static BinderServices connect() throws BridgeFailure {
        return new BinderServices(
                connectService(
                        "package",
                        "android.content.pm.IPackageManager$Stub",
                        "package"),
                connectService("user", "android.os.IUserManager$Stub", "device"));
    }

    Object packageManager() {
        return packageManager;
    }

    Object userManager() {
        return userManager;
    }

    @SuppressLint("PrivateApi")
    private static Object connectService(String serviceName, String stubName, String requestId)
            throws BridgeFailure {
        try {
            Class<?> serviceManager = Class.forName("android.os.ServiceManager");
            Method getService = serviceManager.getMethod("getService", String.class);
            Object binder = getService.invoke(null, serviceName);
            if (binder == null) {
                throw new BridgeFailure(requestId, ErrorCode.INTERNAL);
            }
            Class<?> binderType = Class.forName("android.os.IBinder");
            Class<?> stubType = Class.forName(stubName);
            Method asInterface = stubType.getMethod("asInterface", binderType);
            Object service = asInterface.invoke(null, binder);
            if (service == null) {
                throw new BridgeFailure(requestId, ErrorCode.INTERNAL);
            }
            return service;
        } catch (BridgeFailure failure) {
            throw failure;
        } catch (InvocationTargetException failure) {
            throw new BridgeFailure(requestId, ErrorCode.INTERNAL);
        } catch (ReflectiveOperationException | RuntimeException | LinkageError failure) {
            throw new BridgeFailure(requestId, ErrorCode.INTERNAL);
        }
    }
}
