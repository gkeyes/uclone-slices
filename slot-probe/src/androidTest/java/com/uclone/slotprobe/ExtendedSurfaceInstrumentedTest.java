package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.net.Uri;
import android.os.Bundle;
import android.os.Process;
import android.os.SystemClock;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;

import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class ExtendedSurfaceInstrumentedTest {
    private static final Uri URI = Uri.parse("content://" + ProbeContract.AUTHORITY);

    @Test
    public void boundedWorkerReportsRemoteProcessIdentity() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        Bundle started = call(controller, ProbeContract.METHOD_WORKER_START, null, request());
        try {
            Bundle read = call(controller, ProbeContract.METHOD_WORKER_READ, null, request());
            assertTrue(started.getBoolean("workerBound"));
            assertTrue(read.getBoolean("workerBound"));
            assertEquals("com.uclone.slotprobe:worker", read.getString("process"));
            assertNotEquals(Process.myPid(), read.getInt("pid"));
        } finally {
            Bundle stopped = call(
                    controller,
                    ProbeContract.METHOD_WORKER_STOP,
                    null,
                    request()
            );
            assertFalse(stopped.getBoolean("workerBound"));
        }
    }

    @Test
    public void workerReadReconnectsAfterBindingIsReleased() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        assertTrue(call(controller, ProbeContract.METHOD_WORKER_START, null, request())
                .getBoolean("workerBound"));
        assertFalse(call(controller, ProbeContract.METHOD_WORKER_STOP, null, request())
                .getBoolean("workerBound"));
        try {
            assertTrue(call(controller, ProbeContract.METHOD_WORKER_READ, null, request())
                    .getBoolean("workerBound"));
        } finally {
            call(controller, ProbeContract.METHOD_WORKER_STOP, null, request());
        }
    }

    @Test
    public void workerReadReconnectsAfterWorkerProcessDeath() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        Bundle started = call(controller, ProbeContract.METHOD_WORKER_START, null, request());
        int originalPid = started.getInt("pid");
        assertTrue(started.getBoolean("workerBound"));
        try {
            Process.killProcess(originalPid);
            SystemClock.sleep(250);
            Bundle rebound = call(controller, ProbeContract.METHOD_WORKER_READ, null, request());
            assertTrue(rebound.getBoolean("workerBound"));
            assertNotEquals(originalPid, rebound.getInt("pid"));
        } finally {
            call(controller, ProbeContract.METHOD_WORKER_STOP, null, request());
        }
    }

    @Test
    public void noNetworkWebViewPersistsCeMarker() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        Bundle written = call(
                controller,
                ProbeContract.METHOD_WEBVIEW_WRITE,
                "WEB_INSTRUMENTED",
                request()
        );
        Bundle read = call(controller, ProbeContract.METHOD_WEBVIEW_READ, null, request());

        assertEquals("WEB_INSTRUMENTED", written.getString("webViewMarker"));
        assertEquals("WEB_INSTRUMENTED", read.getString("webViewMarker"));
        assertTrue(read.getBoolean("webViewNetworkBlocked"));
    }

    @Test
    public void schedulerSurfacesScheduleReadAndCancel() {
        Context controller = InstrumentationRegistry.getInstrumentation().getContext();
        Bundle delayed = request();
        delayed.putInt(ProbeContract.EXTRA_DELAY_SECONDS, 60);

        assertTrue(call(
                controller,
                ProbeContract.METHOD_JOB_SCHEDULE,
                "JOB_INSTRUMENTED",
                delayed
        ).getBoolean("jobPending"));
        assertTrue(call(
                controller,
                ProbeContract.METHOD_JOB_READ,
                null,
                request()
        ).getBoolean("jobPending"));
        assertFalse(call(
                controller,
                ProbeContract.METHOD_JOB_CANCEL,
                null,
                request()
        ).getBoolean("jobPending"));

        assertTrue(call(
                controller,
                ProbeContract.METHOD_ALARM_SCHEDULE,
                "ALARM_INSTRUMENTED",
                delayed
        ).getBoolean("alarmPending"));
        assertFalse(call(
                controller,
                ProbeContract.METHOD_ALARM_CANCEL,
                null,
                request()
        ).getBoolean("alarmPending"));
    }

    private static Bundle call(Context context, String method, String arg, Bundle extras) {
        return context.getContentResolver().call(URI, method, arg, extras);
    }

    private static Bundle request() {
        return ProbeContract.request();
    }
}
