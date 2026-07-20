package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;

import org.junit.Test;

public final class MutationPolicyTest {
    @Test
    public void enabledMutationUsesApi36SynchronousPersistenceFlag() {
        assertEquals(0x2, MutationPolicy.SYNCHRONOUS_ENABLED_FLAGS);
    }
}
