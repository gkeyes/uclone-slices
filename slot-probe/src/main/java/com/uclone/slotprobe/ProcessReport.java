package com.uclone.slotprobe;

import android.app.Application;
import android.content.Context;
import android.os.Bundle;
import android.os.Process;
import android.os.SystemClock;

final class ProcessReport {
    private ProcessReport() {
    }

    static void addTo(Bundle result, Context context) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        result.putString("package", context.getPackageName());
        result.putString("process", Application.getProcessName());
        result.putInt("pid", Process.myPid());
        result.putInt("uid", Process.myUid());
        result.putLong("elapsedRealtimeMs", SystemClock.elapsedRealtime());
        result.putString("dataDir", ProbeFileIo.canonicalPath(context.getDataDir()));
        result.putString("ceFilesDir", ProbeFileIo.canonicalPath(context.getFilesDir()));
        result.putString("deDataDir", ProbeFileIo.canonicalPath(deviceContext.getDataDir()));
    }
}
