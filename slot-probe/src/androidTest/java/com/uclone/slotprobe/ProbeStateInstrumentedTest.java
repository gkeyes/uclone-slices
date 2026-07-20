package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.net.Uri;
import android.os.Bundle;

import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;

import org.junit.Test;
import org.junit.runner.RunWith;

import java.io.File;

@RunWith(AndroidJUnit4.class)
public final class ProbeStateInstrumentedTest {
    @Test
    public void sameSignatureControllerCanCallProtectedProvider() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();

        Bundle result = controller.getContentResolver().call(
                Uri.parse("content://" + ProbeContract.AUTHORITY),
                ProbeContract.METHOD_WRITE,
                "SIGNED_A",
                ProbeContract.request()
        );

        assertEquals("SIGNED_A", result.getString("ceMarker"));
        assertEquals(ProbeContract.VERSION, result.getInt(ProbeContract.EXTRA_CONTRACT_VERSION));
    }

    @Test
    public void ceDePreferencesAndWalRoundTrip() {
        Context context = ApplicationProvider.getApplicationContext();

        ProbeStateStore.write(context, "INSTRUMENTED_A");
        Bundle result = ProbeStateStore.read(context, ProbeContract.METHOD_READ);

        assertEquals("INSTRUMENTED_A", result.getString("ceMarker"));
        assertEquals("INSTRUMENTED_A", result.getString("deMarker"));
        assertEquals("INSTRUMENTED_A", result.getString("preferencesMarker"));
        assertEquals("INSTRUMENTED_A", result.getString("databaseMarker"));
        assertTrue(result.getLong("databaseGeneration") > 0);
    }

    @Test
    public void nativeFixedFilesAndMmapRoundTrip() {
        Context context = ApplicationProvider.getApplicationContext();

        Bundle result = NativeProbe.write(context, "NATIVE_A");

        assertEquals("NATIVE_A", result.getString("nativeCeMarker"));
        assertEquals("NATIVE_A", result.getString("nativeDeMarker"));
        assertEquals("NATIVE_A", result.getString("nativeMmapMarker"));
    }

    @Test
    public void nativeReadFailureDoesNotReturnAnOkBundle() {
        Context context = ApplicationProvider.getApplicationContext();
        NativeProbe.write(context, "NATIVE_SETUP");
        File marker = new File(
                new File(context.getFilesDir(), "native-probe"),
                "native-slot-marker.txt"
        );
        assertTrue(marker.delete());
        assertTrue(marker.mkdir());
        try {
            NativeProbe.read(context, ProbeContract.METHOD_NATIVE_READ);
            throw new AssertionError("Native read should reject a directory marker");
        } catch (IllegalStateException expected) {
            assertTrue(expected.getMessage() != null);
        } finally {
            assertTrue(marker.delete());
            NativeProbe.write(context, "NATIVE_RECOVERED");
        }
    }
}
