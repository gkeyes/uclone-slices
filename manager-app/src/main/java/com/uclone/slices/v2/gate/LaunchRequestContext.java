package com.uclone.slices.v2.gate;

/** Result ownership belongs to one nested start request, including exceptional exits. */
final class LaunchRequestContext {
    private final ThreadLocal<Boolean> owned = new ThreadLocal<>();

    @FunctionalInterface
    interface Invocation { Object proceed() throws Throwable; }

    boolean isOwned() { return Boolean.TRUE.equals(owned.get()); }

    Object during(boolean value, Invocation invocation) throws Throwable {
        Boolean previous = owned.get();
        owned.set(value);
        try { return invocation.proceed(); }
        finally {
            if (previous == null) owned.remove(); else owned.set(previous);
        }
    }
}
