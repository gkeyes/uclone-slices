package com.uclone.slotprobe;

import static org.junit.Assert.assertFalse;

import android.os.Bundle;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class WorkerClientTest {
    @Test
    public void readBeforeBindingReportsUnavailable() {
        Bundle result = WorkerClient.read();

        assertFalse(result.getBoolean("workerBound"));
    }
}
