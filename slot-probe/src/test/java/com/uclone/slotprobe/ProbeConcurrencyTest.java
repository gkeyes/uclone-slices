package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.os.Bundle;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class ProbeConcurrencyTest {
    private SlotProbeProvider provider;

    @Before
    public void createProvider() {
        provider = Robolectric.buildContentProvider(SlotProbeProvider.class).create().get();
    }

    @Test
    public void concurrentWritesNeverReturnMixedState() throws Exception {
        ExecutorService executor = Executors.newFixedThreadPool(2);
        try {
            Future<?> first = executor.submit(() -> writeRepeatedly("CONCURRENT_A"));
            Future<?> second = executor.submit(() -> writeRepeatedly("CONCURRENT_B"));
            first.get();
            second.get();

            Bundle finalState = provider.call(
                    ProbeContract.METHOD_READ,
                    null,
                    ProbeContract.request()
            );
            String marker = finalState.getString("ceMarker");
            assertTrue("CONCURRENT_A".equals(marker) || "CONCURRENT_B".equals(marker));
            assertConsistent(finalState, marker);
        } finally {
            executor.shutdownNow();
        }
    }

    @Test
    public void concurrentWalStressWritesVerifyWalMode() throws Exception {
        ExecutorService executor = Executors.newFixedThreadPool(2);
        try {
            Future<Bundle> first = executor.submit(() -> stress("WAL_CONCURRENT_A"));
            Future<Bundle> second = executor.submit(() -> stress("WAL_CONCURRENT_B"));

            assertTrue(first.get().getBoolean("walEnabled"));
            assertTrue(second.get().getBoolean("walEnabled"));
            assertEquals("wal", first.get().getString("journalMode").toLowerCase());
            assertEquals("wal", second.get().getString("journalMode").toLowerCase());
        } finally {
            executor.shutdownNow();
        }
    }

    private void writeRepeatedly(String marker) {
        for (int index = 0; index < 20; index++) {
            Bundle result = provider.call(
                    ProbeContract.METHOD_WRITE,
                    marker,
                    ProbeContract.request()
            );
            assertConsistent(result, marker);
        }
    }

    private Bundle stress(String marker) {
        Bundle request = ProbeContract.request();
        request.putInt(ProbeContract.EXTRA_ITERATIONS, 4);
        return provider.call(ProbeContract.METHOD_WAL_STRESS, marker, request);
    }

    private static void assertConsistent(Bundle result, String marker) {
        assertEquals(marker, result.getString("ceMarker"));
        assertEquals(marker, result.getString("deMarker"));
        assertEquals(marker, result.getString("preferencesMarker"));
        assertEquals(marker, result.getString("databaseMarker"));
    }
}
