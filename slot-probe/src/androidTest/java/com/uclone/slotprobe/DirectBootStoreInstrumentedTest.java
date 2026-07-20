package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.content.Intent;
import android.os.Bundle;

import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;

import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class DirectBootStoreInstrumentedTest {
    @Test
    public void directBootReceiverWritesOnlyDeviceProtectedStore() {
        Context context = ApplicationProvider.getApplicationContext();
        DirectBootProbeReceiver receiver = new DirectBootProbeReceiver();
        ProbeStateStore.write(context, "CE_UNCHANGED");

        receiver.onReceive(context, new Intent(Intent.ACTION_LOCKED_BOOT_COMPLETED));
        Bundle result = ScheduledProbeStore.readDirectBoot(context);
        Bundle ceState = ProbeStateStore.read(context, ProbeContract.METHOD_READ);

        assertTrue(result.getBoolean("deviceProtectedStorage"));
        assertEquals(Intent.ACTION_LOCKED_BOOT_COMPLETED, result.getString("directBootAction"));
        assertEquals("CE_UNCHANGED", ceState.getString("ceMarker"));
        assertEquals("CE_UNCHANGED", ceState.getString("preferencesMarker"));
        assertEquals("CE_UNCHANGED", ceState.getString("databaseMarker"));
    }
}
