package com.uclone.slices.v2.gate;

import org.junit.Test;
import java.lang.reflect.InvocationTargetException;
import static org.junit.Assert.*;

public class FrameworkAccessTest {
    private static class Parent {
        private int selected = 7;
        private String lookup(int value) { return "int:" + value; }
        private String lookup(long value) { return "long:" + value; }
        private static Object token(Object token) { return token; }
        private void fail() { throw new IllegalStateException("framework failure"); }
    }

    private static final class Child extends Parent { }

    @Test public void inheritedPrivateMembersKeepExactPrimitiveSignatures() throws Exception {
        Child receiver = new Child();
        assertEquals(7, FrameworkAccess.get(receiver, "selected"));
        FrameworkAccess.set(receiver, "selected", 11);
        assertEquals(11, FrameworkAccess.get(receiver, "selected"));
        assertEquals("int:4", FrameworkAccess.call(receiver, "lookup", new Class<?>[]{int.class}, 4));
        assertEquals("long:4", FrameworkAccess.call(receiver, "lookup", new Class<?>[]{long.class}, 4L));
        assertThrows(NoSuchMethodException.class,
            () -> FrameworkAccess.call(receiver, "lookup", new Class<?>[]{Integer.class}, 4));
    }

    @Test public void staticCallsAndTargetFailuresRemainDistinctFromMissingMembers() throws Exception {
        Object token = new Object();
        assertSame(token, FrameworkAccess.callStatic(Child.class, "token", new Class<?>[]{Object.class}, token));
        InvocationTargetException failed = assertThrows(InvocationTargetException.class,
            () -> FrameworkAccess.call(new Child(), "fail"));
        assertTrue(failed.getCause() instanceof IllegalStateException);
        assertThrows(NoSuchFieldException.class, () -> FrameworkAccess.get(new Child(), "absent"));
    }

    @Test public void requiredMembersAreCheckedWithoutAReceiver() throws Exception {
        FrameworkAccess.requireFields(Child.class, "selected");
        FrameworkAccess.requireMethod(Child.class, "lookup", long.class);
        assertThrows(NoSuchFieldException.class, () -> FrameworkAccess.requireFields(Child.class, "selected", "absent"));
        assertThrows(NoSuchMethodException.class, () -> FrameworkAccess.requireMethod(Child.class, "lookup"));
    }
}
