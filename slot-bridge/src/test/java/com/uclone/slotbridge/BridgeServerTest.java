package com.uclone.slotbridge;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class BridgeServerTest {
    @Test
    public void acceptsOnlyExactServeArgv() {
        assertTrue(BridgeServer.isServeRequest(new String[] {"serve"}));
        assertFalse(BridgeServer.isServeRequest(new String[] {"serve", "extra"}));
        assertFalse(BridgeServer.isServeRequest(new String[] {}));
        assertFalse(BridgeServer.isServeRequest(null));
    }

    @Test
    public void parsesOnlyBoundedTabSeparatedArgv() throws Exception {
        assertArrayEquals(
                new String[] {"probe-gate", TargetProfile.PACKAGE},
                BridgeServer.parseLine("probe-gate\t" + TargetProfile.PACKAGE));
        assertThrows(BridgeFailure.class, () -> BridgeServer.parseLine(""));
        assertThrows(BridgeFailure.class, () -> BridgeServer.parseLine("probe-gate\t"));
        assertThrows(
                BridgeFailure.class,
                () -> BridgeServer.parseLine("x".repeat(1025)));
    }
}
