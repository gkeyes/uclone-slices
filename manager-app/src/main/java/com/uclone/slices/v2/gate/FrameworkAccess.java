package com.uclone.slices.v2.gate;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.ConcurrentHashMap;

/** Exact framework members; no dependency on the legacy Xposed helpers. */
final class FrameworkAccess {
    private static final Class<?>[] NO_ARGUMENTS = new Class<?>[0];
    private static final ConcurrentHashMap<FieldKey, Field> FIELDS = new ConcurrentHashMap<>();
    private static final ConcurrentHashMap<MethodKey, Method> METHODS = new ConcurrentHashMap<>();

    private record FieldKey(Class<?> owner, String name) { }
    private record MethodKey(Class<?> owner, String name, List<Class<?>> parameters) { }

    private FrameworkAccess() { }

    static Object get(Object receiver, String name) throws ReflectiveOperationException {
        return field(receiver.getClass(), name).get(receiver);
    }

    static void set(Object receiver, String name, Object value) throws ReflectiveOperationException {
        field(receiver.getClass(), name).set(receiver, value);
    }

    static Object call(Object receiver, String name) throws ReflectiveOperationException {
        return call(receiver, name, NO_ARGUMENTS);
    }

    static Object call(Object receiver, String name, Class<?>[] types, Object... arguments)
            throws ReflectiveOperationException {
        return method(receiver.getClass(), name, types).invoke(receiver, arguments);
    }

    static Object callStatic(Class<?> owner, String name, Class<?>[] types, Object... arguments)
            throws ReflectiveOperationException {
        return method(owner, name, types).invoke(null, arguments);
    }

    /** Fails unless every named field exists on {@code owner} or one of its superclasses. */
    static void requireFields(Class<?> owner, String... names) throws NoSuchFieldException {
        for (String name : names) field(owner, name);
    }

    static void requireMethod(Class<?> owner, String name, Class<?>... parameters) throws NoSuchMethodException {
        method(owner, name, parameters);
    }

    private static Field field(Class<?> owner, String name) throws NoSuchFieldException {
        FieldKey key = new FieldKey(owner, name);
        Field cached = FIELDS.get(key);
        if (cached != null) return cached;
        for (Class<?> type = owner; type != null; type = type.getSuperclass()) {
            try {
                Field found = type.getDeclaredField(name);
                found.setAccessible(true);
                Field previous = FIELDS.putIfAbsent(key, found);
                return previous == null ? found : previous;
            } catch (NoSuchFieldException absent) { /* Try the framework superclass. */ }
        }
        throw new NoSuchFieldException(owner.getName() + "." + name);
    }

    private static Method method(Class<?> owner, String name, Class<?>[] parameters) throws NoSuchMethodException {
        MethodKey key = new MethodKey(owner, name, Collections.unmodifiableList(Arrays.asList(parameters.clone())));
        Method cached = METHODS.get(key);
        if (cached != null) return cached;
        for (Class<?> type = owner; type != null; type = type.getSuperclass()) {
            try {
                Method found = type.getDeclaredMethod(name, parameters);
                found.setAccessible(true);
                Method previous = METHODS.putIfAbsent(key, found);
                return previous == null ? found : previous;
            } catch (NoSuchMethodException absent) { /* Keep overload matching exact. */ }
        }
        throw new NoSuchMethodException(owner.getName() + "." + name);
    }
}
