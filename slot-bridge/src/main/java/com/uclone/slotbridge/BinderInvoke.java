package com.uclone.slotbridge;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

final class BinderInvoke {
    private BinderInvoke() {}

    static Object call(
            Object target,
            String requestId,
            ErrorCode failureCode,
            String methodName,
            Class<?>[] parameterTypes,
            Object... arguments)
            throws BridgeFailure {
        if (target == null) {
            throw new BridgeFailure(requestId, failureCode);
        }
        try {
            Method method = findMethod(target.getClass(), methodName, parameterTypes);
            return method.invoke(target, arguments);
        } catch (InvocationTargetException failure) {
            throw new BridgeFailure(requestId, failureCode);
        } catch (ReflectiveOperationException | RuntimeException | LinkageError failure) {
            throw new BridgeFailure(requestId, failureCode);
        }
    }

    private static Method findMethod(
            Class<?> targetType, String methodName, Class<?>[] parameterTypes)
            throws NoSuchMethodException {
        for (Class<?> current = targetType; current != null; current = current.getSuperclass()) {
            for (Class<?> interfaceType : current.getInterfaces()) {
                try {
                    return interfaceType.getMethod(methodName, parameterTypes);
                } catch (NoSuchMethodException ignored) {
                    Method nested = findInterfaceMethod(interfaceType, methodName, parameterTypes);
                    if (nested != null) {
                        return nested;
                    }
                }
            }
        }
        return targetType.getMethod(methodName, parameterTypes);
    }

    private static Method findInterfaceMethod(
            Class<?> interfaceType, String methodName, Class<?>[] parameterTypes) {
        for (Class<?> parent : interfaceType.getInterfaces()) {
            try {
                return parent.getMethod(methodName, parameterTypes);
            } catch (NoSuchMethodException ignored) {
                Method nested = findInterfaceMethod(parent, methodName, parameterTypes);
                if (nested != null) {
                    return nested;
                }
            }
        }
        return null;
    }
}
