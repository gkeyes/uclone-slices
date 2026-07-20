package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import android.os.Bundle;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class ProbeContractTest {
    @Test
    public void requestCarriesCurrentContractVersion() {
        Bundle request = ProbeContract.request();

        ProbeContract.requireRequest(request);

        assertEquals(
                ProbeContract.VERSION,
                request.getInt(ProbeContract.EXTRA_CONTRACT_VERSION)
        );
    }

    @Test
    public void requestRejectsUnknownFields() {
        Bundle request = ProbeContract.request();
        request.putString("path", "/data/user/0/other.package");

        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireRequest(request)
        );
    }

    @Test
    public void markerRejectsPathsAndShellSyntax() {
        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireMarker("../../other")
        );
        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireMarker("A;id")
        );
        assertEquals("SAFE_A-1", ProbeContract.requireMarker("SAFE_A-1"));
    }

    @Test
    public void noArgContractRejectsIgnoredInput() {
        ProbeContract.requireNoArg(null);

        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireNoArg("ignored")
        );
    }

    @Test
    public void boundedInputsRejectExcessiveWork() {
        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireBoundedInt(
                        ProbeContract.EXTRA_ITERATIONS,
                        ProbeContract.MAX_STRESS_ITERATIONS + 1,
                        1,
                        ProbeContract.MAX_STRESS_ITERATIONS
                )
        );
        assertThrows(
                IllegalArgumentException.class,
                () -> ProbeContract.requireBoundedArg(
                        "hold",
                        "61",
                        1,
                        ProbeContract.MAX_HOLD_SECONDS
                )
        );
    }
}
