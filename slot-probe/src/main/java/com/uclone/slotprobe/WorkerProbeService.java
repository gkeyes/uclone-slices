package com.uclone.slotprobe;

import android.app.Service;
import android.content.Intent;
import android.os.Bundle;
import android.os.IBinder;

public final class WorkerProbeService extends Service {
    private final IWorkerProbe.Stub binder = new IWorkerProbe.Stub() {
        @Override
        public Bundle readIdentity() {
            Bundle result = ProbeStateStore.read(
                    WorkerProbeService.this,
                    ProbeContract.METHOD_WORKER_READ
            );
            result.putBoolean("workerBound", true);
            return result;
        }
    };

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }
}
