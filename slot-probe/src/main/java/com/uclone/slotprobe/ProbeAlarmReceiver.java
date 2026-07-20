package com.uclone.slotprobe;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

public final class ProbeAlarmReceiver extends BroadcastReceiver {
    static final String ACTION_RUN = "com.uclone.slotprobe.action.RUN_ALARM";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!ACTION_RUN.equals(intent.getAction())) {
            return;
        }
        String marker = ProbeContract.requireMarker(
                intent.getStringExtra(ProbeScheduler.EXTRA_MARKER)
        );
        ScheduledProbeStore.record(
                context.createDeviceProtectedStorageContext(),
                "alarm",
                marker
        );
    }
}
