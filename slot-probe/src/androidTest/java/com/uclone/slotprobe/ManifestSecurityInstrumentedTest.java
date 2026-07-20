package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.content.ComponentName;
import android.content.Context;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.pm.ProviderInfo;

import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;

import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class ManifestSecurityInstrumentedTest {
    @Test
    public void providersRequirePermissionAndQaActivityIsLaunchable() throws Exception {
        Context context = ApplicationProvider.getApplicationContext();
        PackageManager packageManager = context.getPackageManager();
        ProviderInfo mainProvider = packageManager.resolveContentProvider(
                ProbeContract.AUTHORITY,
                PackageManager.ComponentInfoFlags.of(0)
        );
        ProviderInfo remoteProvider = packageManager.resolveContentProvider(
                ProbeContract.REMOTE_AUTHORITY,
                PackageManager.ComponentInfoFlags.of(0)
        );
        ActivityInfo activity = packageManager.getActivityInfo(
                new ComponentName(context, ManualQaActivity.class),
                PackageManager.ComponentInfoFlags.of(0)
        );

        assertEquals(ProbeContract.CONTROL_PERMISSION, mainProvider.readPermission);
        assertEquals(ProbeContract.CONTROL_PERMISSION, mainProvider.writePermission);
        assertEquals(ProbeContract.CONTROL_PERMISSION, remoteProvider.readPermission);
        assertEquals(ProbeContract.CONTROL_PERMISSION, remoteProvider.writePermission);
        assertNull(activity.permission);
        assertTrue(activity.exported);
    }
}
