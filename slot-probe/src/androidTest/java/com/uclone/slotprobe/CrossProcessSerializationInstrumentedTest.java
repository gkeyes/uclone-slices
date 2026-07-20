package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.net.Uri;
import android.os.Bundle;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;

import org.junit.Test;
import org.junit.runner.RunWith;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

@RunWith(AndroidJUnit4.class)
public final class CrossProcessSerializationInstrumentedTest {
    @Test
    public void mainAndRemoteProviderWritesRemainCoherent() throws Exception {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        ExecutorService executor = Executors.newFixedThreadPool(2);
        try {
            Future<?> main = executor.submit(() -> writeRepeatedly(
                    controller,
                    ProbeContract.AUTHORITY,
                    "PROCESS_MAIN"
            ));
            Future<?> remote = executor.submit(() -> writeRepeatedly(
                    controller,
                    ProbeContract.REMOTE_AUTHORITY,
                    "PROCESS_REMOTE"
            ));
            main.get();
            remote.get();

            Bundle finalState = call(controller, ProbeContract.AUTHORITY, null);
            String marker = finalState.getString("ceMarker");
            assertTrue("PROCESS_MAIN".equals(marker) || "PROCESS_REMOTE".equals(marker));
            assertConsistent(finalState, marker);
        } finally {
            executor.shutdownNow();
        }
    }

    private static void writeRepeatedly(Context context, String authority, String marker) {
        for (int index = 0; index < 25; index++) {
            assertConsistent(call(context, authority, marker), marker);
        }
    }

    private static Bundle call(Context context, String authority, String marker) {
        return context.getContentResolver().call(
                Uri.parse("content://" + authority),
                marker == null ? ProbeContract.METHOD_READ : ProbeContract.METHOD_WRITE,
                marker,
                ProbeContract.request()
        );
    }

    private static void assertConsistent(Bundle result, String marker) {
        assertEquals(marker, result.getString("ceMarker"));
        assertEquals(marker, result.getString("deMarker"));
        assertEquals(marker, result.getString("preferencesMarker"));
        assertEquals(marker, result.getString("databaseMarker"));
        assertEquals(result.getString("ceTransactionId"), result.getString("deTransactionId"));
        assertEquals(
                result.getString("ceTransactionId"),
                result.getString("preferencesTransactionId")
        );
        assertEquals(
                result.getString("ceTransactionId"),
                result.getString("databaseTransactionId")
        );
    }
}
