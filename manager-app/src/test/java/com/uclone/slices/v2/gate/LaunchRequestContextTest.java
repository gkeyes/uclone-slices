package com.uclone.slices.v2.gate;

import org.junit.Test;
import static org.junit.Assert.*;

public class LaunchRequestContextTest {
    @Test public void nestedStartsRestoreOwnershipAfterSuccessAndFailure() throws Throwable {
        LaunchRequestContext context = new LaunchRequestContext();
        assertFalse(context.isOwned());
        Object answer = context.during(true, () -> {
            assertTrue(context.isOwned());
            assertEquals("inner", context.during(false, () -> {
                assertFalse(context.isOwned());
                return "inner";
            }));
            assertTrue(context.isOwned());
            IllegalStateException failure = new IllegalStateException("OS rejected request");
            Throwable thrown = assertThrows(IllegalStateException.class, () -> context.during(false, () -> {
                assertFalse(context.isOwned());
                throw failure;
            }));
            assertSame(failure, thrown);
            assertTrue(context.isOwned());
            return "outer";
        });
        assertEquals("outer", answer);
        assertFalse(context.isOwned());
    }

    @Test public void failedRequestsCannotAuthorizeTheNextRequest() {
        LaunchRequestContext context = new LaunchRequestContext();
        assertThrows(IllegalArgumentException.class, () -> context.during(true, () -> {
            throw new IllegalArgumentException("failed");
        }));
        assertFalse(context.isOwned());
    }
}
