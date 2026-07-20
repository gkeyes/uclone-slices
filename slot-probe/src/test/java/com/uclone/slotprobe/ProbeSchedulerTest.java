package com.uclone.slotprobe;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.os.Bundle;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class ProbeSchedulerTest {
    private Context context;

    @Before
    public void setUp() {
        context = RuntimeEnvironment.getApplication();
        ProbeScheduler.cancelJob(context);
        ProbeScheduler.cancelAlarm(context);
    }

    @After
    public void tearDown() {
        ProbeScheduler.cancelJob(context);
        ProbeScheduler.cancelAlarm(context);
    }

    @Test
    public void jobReadUsesJobSchedulerPendingState() {
        Bundle scheduled = ProbeScheduler.scheduleJob(context, "JOB_TEST", 60);
        Bundle cancelled = ProbeScheduler.cancelJob(context);

        assertTrue(scheduled.getBoolean("jobPending"));
        assertFalse(cancelled.getBoolean("jobPending"));
    }

    @Test
    public void jobReadDoesNotUseStaleDeviceStateForPendingFlag() {
        ProbeScheduler.scheduleJob(context, "JOB_STALE", 60);
        ScheduledProbeStore.setPending(context, "job", false);

        assertTrue(ProbeScheduler.readJob(context).getBoolean("jobPending"));
    }

    @Test
    public void alarmScheduleAndCancelPersistPendingState() {
        Bundle scheduled = ProbeScheduler.scheduleAlarm(context, "ALARM_TEST", 60);
        Bundle cancelled = ProbeScheduler.cancelAlarm(context);

        assertTrue(scheduled.getBoolean("alarmPending"));
        assertFalse(cancelled.getBoolean("alarmPending"));
    }
}
