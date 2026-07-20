package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import android.os.Bundle;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class SlotProbeProviderTest {
    private SlotProbeProvider provider;

    @Before
    public void createProvider() {
        provider = Robolectric.buildContentProvider(SlotProbeProvider.class).create().get();
    }

    @Test
    public void writePreservesCeDePreferencesAndWalContract() {
        Bundle written = provider.call(
                ProbeContract.METHOD_WRITE,
                "TEST_A",
                ProbeContract.request()
        );
        Bundle read = provider.call(
                ProbeContract.METHOD_READ,
                null,
                ProbeContract.request()
        );

        assertEquals("TEST_A", written.getString("ceMarker"));
        assertEquals("TEST_A", read.getString("deMarker"));
        assertEquals("TEST_A", read.getString("preferencesMarker"));
        assertEquals("TEST_A", read.getString("databaseMarker"));
        assertEquals(ProbeContract.VERSION, read.getInt(ProbeContract.EXTRA_CONTRACT_VERSION));
        assertEquals("com.uclone.slotprobe", read.getString("package"));
    }

    @Test
    public void walStressIsBoundedAndAdvancesGeneration() {
        provider.call(ProbeContract.METHOD_WRITE, "WAL_A", ProbeContract.request());
        long initialGeneration = provider.call(
                ProbeContract.METHOD_READ,
                null,
                ProbeContract.request()
        ).getLong("databaseGeneration");
        Bundle request = ProbeContract.request();
        request.putInt(ProbeContract.EXTRA_ITERATIONS, 3);

        Bundle result = provider.call(ProbeContract.METHOD_WAL_STRESS, "WAL_B", request);

        assertEquals("WAL_B", result.getString("databaseMarker"));
        assertEquals(initialGeneration + 3, result.getLong("databaseGeneration"));
        assertEquals(3, result.getInt(ProbeContract.EXTRA_ITERATIONS));
        assertTrue(result.getBoolean("walEnabled"));
        assertEquals("wal", result.getString("journalMode").toLowerCase());
    }

    @Test
    public void callsWithoutVersionAreRejected() {
        assertThrows(
                IllegalArgumentException.class,
                () -> provider.call(ProbeContract.METHOD_READ, null, null)
        );
    }

    @Test
    public void unknownMethodIsRejected() {
        assertThrows(
                IllegalArgumentException.class,
                () -> provider.call("shell", "id", ProbeContract.request())
        );
    }

    @Test
    public void noArgMethodRejectsIgnoredArg() {
        assertThrows(
                IllegalArgumentException.class,
                () -> provider.call(
                        ProbeContract.METHOD_READ,
                        "ignored",
                        ProbeContract.request()
                )
        );
        assertThrows(
                IllegalArgumentException.class,
                () -> provider.call(
                        ProbeContract.METHOD_WORKER_READ,
                        "ignored",
                        ProbeContract.request()
                )
        );
    }

    @Test
    public void everyNoArgMethodRejectsPathAndCommandArgs() {
        String[] methods = {
                ProbeContract.METHOD_READ,
                ProbeContract.METHOD_WORKER_START,
                ProbeContract.METHOD_WORKER_READ,
                ProbeContract.METHOD_WORKER_STOP,
                ProbeContract.METHOD_NATIVE_READ,
                ProbeContract.METHOD_WEBVIEW_READ,
                ProbeContract.METHOD_JOB_CANCEL,
                ProbeContract.METHOD_JOB_READ,
                ProbeContract.METHOD_ALARM_CANCEL,
                ProbeContract.METHOD_ALARM_READ,
                ProbeContract.METHOD_DIRECT_BOOT_READ
        };
        for (String method : methods) {
            assertThrows(
                    method,
                    IllegalArgumentException.class,
                    () -> provider.call(method, "/data/other;id", ProbeContract.request())
            );
        }
    }

    @Test
    public void preparedStateTransactionCannotReportOk() {
        ProbeStateStore.write(RuntimeEnvironment.getApplication(), "COMMITTED_A");
        ProbeTransaction.begin(
                RuntimeEnvironment.getApplication(),
                ProbeTransaction.STATE_SCOPE,
                "PREPARED_B"
        );

        assertThrows(
                IllegalStateException.class,
                () -> provider.call(
                        ProbeContract.METHOD_READ,
                        null,
                        ProbeContract.request()
                )
        );
    }
}
