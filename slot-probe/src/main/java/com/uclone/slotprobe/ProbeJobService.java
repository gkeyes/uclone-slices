package com.uclone.slotprobe;

import android.app.job.JobParameters;
import android.app.job.JobService;

public final class ProbeJobService extends JobService {
    @Override
    public boolean onStartJob(JobParameters params) {
        String marker = ProbeContract.requireMarker(
                params.getExtras().getString(ProbeScheduler.EXTRA_MARKER)
        );
        ScheduledProbeStore.record(
                createDeviceProtectedStorageContext(),
                "job",
                marker
        );
        return false;
    }

    @Override
    public boolean onStopJob(JobParameters params) {
        return false;
    }
}
