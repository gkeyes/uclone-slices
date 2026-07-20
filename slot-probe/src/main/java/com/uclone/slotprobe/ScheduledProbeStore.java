package com.uclone.slotprobe;

import android.content.Context;
import android.os.Bundle;
import android.os.SystemClock;

import java.io.File;

final class ScheduledProbeStore {
    private static final String DIRECTORY_NAME = "scheduled-probe";
    private static final String DIRECT_BOOT_FILE = "direct-boot.state";

    private ScheduledProbeStore() {
    }

    static void record(Context deviceContext, String kind, String marker) {
        requireDeviceContext(deviceContext);
        ProbeContract.requireMarker(marker);
        ProbeOperationLock.run(deviceContext, () -> writeState(
                deviceContext,
                kind,
                new State(marker, SystemClock.elapsedRealtime(), false)
        ));
    }

    static void setPending(Context context, String kind, boolean pending) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        ProbeOperationLock.run(deviceContext, () -> {
            State previous = readState(deviceContext, kind);
            writeState(
                    deviceContext,
                    kind,
                    new State(previous.marker(), previous.elapsedMs(), pending)
            );
        });
    }

    static void recordDirectBoot(Context deviceContext, String action) {
        requireDeviceContext(deviceContext);
        ProbeOperationLock.run(deviceContext, () -> ProbeFileIo.writeSynced(
                stateFile(deviceContext, DIRECT_BOOT_FILE),
                action + "|" + SystemClock.elapsedRealtime()
        ));
    }

    static void addState(Context context, String kind, Bundle result) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        ProbeOperationLock.run(deviceContext, () -> {
            State state = readState(deviceContext, kind);
            result.putString(kind + "Marker", state.marker());
            result.putLong(kind + "ElapsedMs", state.elapsedMs());
            String pendingKey = kind + "Pending";
            if (!result.containsKey(pendingKey)) {
                result.putBoolean(pendingKey, state.pending());
            }
        });
    }

    static Bundle readDirectBoot(Context context) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        return ProbeOperationLock.call(deviceContext, () -> {
            String[] fields = readFields(stateFile(deviceContext, DIRECT_BOOT_FILE), 2);
            Bundle result = ProbeContract.response(ProbeContract.METHOD_DIRECT_BOOT_READ);
            result.putString("directBootAction", fields[0]);
            result.putLong("directBootElapsedMs", parseLong(fields[1]));
            result.putBoolean("deviceProtectedStorage", deviceContext.isDeviceProtectedStorage());
            return result;
        });
    }

    private static State readState(Context deviceContext, String kind) {
        String[] fields = readFields(stateFile(deviceContext, requireKind(kind) + ".state"), 3);
        return new State(fields[0], parseLong(fields[1]), Boolean.parseBoolean(fields[2]));
    }

    private static void writeState(Context deviceContext, String kind, State state) {
        ProbeFileIo.writeSynced(
                stateFile(deviceContext, requireKind(kind) + ".state"),
                state.marker() + "|" + state.elapsedMs() + "|" + state.pending()
        );
    }

    private static String[] readFields(File file, int expectedCount) {
        String value = ProbeFileIo.read(file);
        if ("<missing>".equals(value)) {
            String[] missing = new String[expectedCount];
            missing[0] = "<missing>";
            for (int index = 1; index < expectedCount; index++) missing[index] = "0";
            return missing;
        }
        String[] fields = value.split("\\|", -1);
        if (fields.length != expectedCount) {
            throw new IllegalStateException("Scheduled probe state is malformed");
        }
        return fields;
    }

    private static long parseLong(String value) {
        try {
            return Long.parseLong(value);
        } catch (NumberFormatException error) {
            throw new IllegalStateException("Scheduled probe timestamp is malformed", error);
        }
    }

    private static File stateFile(Context deviceContext, String name) {
        return new File(new File(deviceContext.getFilesDir(), DIRECTORY_NAME), name);
    }

    private static String requireKind(String kind) {
        if (!"job".equals(kind) && !"alarm".equals(kind)) {
            throw new IllegalArgumentException("Unsupported scheduled probe kind");
        }
        return kind;
    }

    private static void requireDeviceContext(Context context) {
        if (!context.isDeviceProtectedStorage()) {
            throw new IllegalArgumentException("Scheduled probe state must use DE storage");
        }
    }

    private record State(String marker, long elapsedMs, boolean pending) {
    }
}
