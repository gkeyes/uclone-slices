package com.uclone.slotprobe;

import android.app.AlarmManager;
import android.app.PendingIntent;
import android.app.job.JobInfo;
import android.app.job.JobScheduler;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.PersistableBundle;
import android.os.SystemClock;

final class ProbeScheduler {
    static final int JOB_ID = 0x5543_4c4f;
    static final int ALARM_REQUEST_CODE = 0x534c_4f54;
    static final String EXTRA_MARKER = "fixed_marker";

    private ProbeScheduler() {
    }

    static Bundle scheduleJob(Context context, String marker, int delaySeconds) {
        ProbeContract.requireMarker(marker);
        int delay = requireDelay(delaySeconds);
        return ProbeOperationLock.call(context, () -> {
            PersistableBundle extras = new PersistableBundle();
            extras.putString(EXTRA_MARKER, marker);
            JobInfo job = new JobInfo.Builder(
                    JOB_ID,
                    new ComponentName(context, ProbeJobService.class)
            ).setMinimumLatency(delay * 1_000L).setExtras(extras).build();
            int resultCode = jobScheduler(context).schedule(job);
            if (resultCode != JobScheduler.RESULT_SUCCESS) {
                throw new IllegalStateException("JobScheduler rejected the probe job");
            }
            Bundle result = readJob(context);
            result.putString(ProbeContract.KEY_OPERATION, ProbeContract.METHOD_JOB_SCHEDULE);
            result.putInt(ProbeContract.EXTRA_DELAY_SECONDS, delay);
            return result;
        });
    }

    static Bundle cancelJob(Context context) {
        return ProbeOperationLock.call(context, () -> {
            jobScheduler(context).cancel(JOB_ID);
            Bundle result = readJob(context);
            result.putString(ProbeContract.KEY_OPERATION, ProbeContract.METHOD_JOB_CANCEL);
            return result;
        });
    }

    static Bundle readJob(Context context) {
        return ProbeOperationLock.call(context, () -> {
            Bundle result = ProbeContract.response(ProbeContract.METHOD_JOB_READ);
            ScheduledProbeStore.addState(context, "job", result);
            result.putBoolean("jobPending", jobScheduler(context).getPendingJob(JOB_ID) != null);
            return result;
        });
    }

    static Bundle scheduleAlarm(Context context, String marker, int delaySeconds) {
        ProbeContract.requireMarker(marker);
        int delay = requireDelay(delaySeconds);
        return ProbeOperationLock.call(context, () -> {
            alarmManager(context).set(
                    AlarmManager.ELAPSED_REALTIME_WAKEUP,
                    SystemClock.elapsedRealtime() + delay * 1_000L,
                    alarmIntent(context, marker)
            );
            ScheduledProbeStore.setPending(context, "alarm", true);
            Bundle result = readAlarm(context);
            result.putString(ProbeContract.KEY_OPERATION, ProbeContract.METHOD_ALARM_SCHEDULE);
            result.putBoolean("alarmScheduled", true);
            result.putInt(ProbeContract.EXTRA_DELAY_SECONDS, delay);
            return result;
        });
    }

    static Bundle cancelAlarm(Context context) {
        return ProbeOperationLock.call(context, () -> {
            alarmManager(context).cancel(alarmIntent(context, "cancelled"));
            ScheduledProbeStore.setPending(context, "alarm", false);
            Bundle result = readAlarm(context);
            result.putString(ProbeContract.KEY_OPERATION, ProbeContract.METHOD_ALARM_CANCEL);
            result.putBoolean("alarmScheduled", false);
            return result;
        });
    }

    static Bundle readAlarm(Context context) {
        return ProbeOperationLock.call(context, () -> {
            Bundle result = ProbeContract.response(ProbeContract.METHOD_ALARM_READ);
            ScheduledProbeStore.addState(context, "alarm", result);
            return result;
        });
    }

    private static int requireDelay(int delaySeconds) {
        return ProbeContract.requireBoundedInt(
                ProbeContract.EXTRA_DELAY_SECONDS,
                delaySeconds,
                1,
                ProbeContract.MAX_SCHEDULE_DELAY_SECONDS
        );
    }

    private static JobScheduler jobScheduler(Context context) {
        return context.getSystemService(JobScheduler.class);
    }

    private static AlarmManager alarmManager(Context context) {
        return context.getSystemService(AlarmManager.class);
    }

    private static PendingIntent alarmIntent(Context context, String marker) {
        Intent intent = new Intent(context, ProbeAlarmReceiver.class)
                .setAction(ProbeAlarmReceiver.ACTION_RUN)
                .putExtra(EXTRA_MARKER, marker);
        return PendingIntent.getBroadcast(
                context,
                ALARM_REQUEST_CODE,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE
        );
    }
}
