package com.uclone.slotpreview.controller;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;

import org.junit.Test;

public final class BoundedOutputTest {
    @Test
    public void keepsAtMostTheConfiguredByteLimitAndDrainsInput() throws Exception {
        byte[] input = "0123456789abcdefghijklmnopqrstuvwxyz".getBytes(StandardCharsets.UTF_8);
        BoundedOutput output = BoundedOutput.read(new ByteArrayInputStream(input), 10);
        assertEquals("0123456789", output.text());
        assertTrue(output.truncated());
    }

    @Test
    public void marksShortOutputAsComplete() throws Exception {
        BoundedOutput output = BoundedOutput.read(
                new ByteArrayInputStream("ok".getBytes(StandardCharsets.UTF_8)),
                10
        );
        assertEquals("ok", output.text());
        assertFalse(output.truncated());
    }

    @Test
    public void rejectsAnUnboundedOrEmptyLimit() {
        assertThrows(
                IllegalArgumentException.class,
                () -> BoundedOutput.read(new ByteArrayInputStream(new byte[0]), 0)
        );
    }
}
