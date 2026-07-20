package com.uclone.slotprobe;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

public final class DirectBootProbeReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        String action = intent.getAction();
        if (!Intent.ACTION_LOCKED_BOOT_COMPLETED.equals(action)
                && !Intent.ACTION_BOOT_COMPLETED.equals(action)) {
            return;
        }
        Context deviceContext = context.createDeviceProtectedStorageContext();
        ScheduledProbeStore.recordDirectBoot(deviceContext, action);
    }
}
