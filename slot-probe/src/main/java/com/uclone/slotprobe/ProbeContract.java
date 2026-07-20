package com.uclone.slotprobe;

import android.os.Bundle;

import java.util.Arrays;
import java.util.HashSet;
import java.util.Set;

public final class ProbeContract {
    public static final int VERSION = 1;
    public static final String AUTHORITY = "com.uclone.slotprobe";
    public static final String REMOTE_AUTHORITY = "com.uclone.slotprobe.remote";
    public static final String CONTROL_PERMISSION = "com.uclone.slotprobe.permission.CONTROL";

    public static final String METHOD_READ = "read";
    public static final String METHOD_WRITE = "write";
    public static final String METHOD_HOLD = "hold";
    public static final String METHOD_WORKER_START = "worker_start";
    public static final String METHOD_WORKER_READ = "worker_read";
    public static final String METHOD_WORKER_STOP = "worker_stop";
    public static final String METHOD_NATIVE_WRITE = "native_write";
    public static final String METHOD_NATIVE_READ = "native_read";
    public static final String METHOD_WEBVIEW_WRITE = "webview_write";
    public static final String METHOD_WEBVIEW_READ = "webview_read";
    public static final String METHOD_WAL_STRESS = "wal_stress";
    public static final String METHOD_JOB_SCHEDULE = "job_schedule";
    public static final String METHOD_JOB_CANCEL = "job_cancel";
    public static final String METHOD_JOB_READ = "job_read";
    public static final String METHOD_ALARM_SCHEDULE = "alarm_schedule";
    public static final String METHOD_ALARM_CANCEL = "alarm_cancel";
    public static final String METHOD_ALARM_READ = "alarm_read";
    public static final String METHOD_DIRECT_BOOT_READ = "direct_boot_read";

    public static final String EXTRA_CONTRACT_VERSION = "contract_version";
    public static final String EXTRA_ITERATIONS = "iterations";
    public static final String EXTRA_DELAY_SECONDS = "delay_seconds";
    public static final String KEY_OPERATION = "operation";
    public static final String KEY_STATUS = "status";
    public static final String KEY_MARKER = "marker";

    public static final int MAX_MARKER_LENGTH = 64;
    public static final int MAX_HOLD_SECONDS = 60;
    public static final int MAX_STRESS_ITERATIONS = 1_000;
    public static final int MAX_SCHEDULE_DELAY_SECONDS = 60;

    private ProbeContract() {
    }

    public static Bundle request() {
        Bundle request = new Bundle();
        request.putInt(EXTRA_CONTRACT_VERSION, VERSION);
        return request;
    }

    public static Bundle response(String operation) {
        Bundle response = new Bundle();
        response.putInt(EXTRA_CONTRACT_VERSION, VERSION);
        response.putString(KEY_OPERATION, operation);
        response.putString(KEY_STATUS, "ok");
        return response;
    }

    public static void requireRequest(Bundle extras, String... methodKeys) {
        if (extras == null || extras.getInt(EXTRA_CONTRACT_VERSION, -1) != VERSION) {
            throw new IllegalArgumentException("contract_version must be " + VERSION);
        }
        Set<String> allowed = new HashSet<>(Arrays.asList(methodKeys));
        allowed.add(EXTRA_CONTRACT_VERSION);
        for (String key : extras.keySet()) {
            if (!allowed.contains(key)) {
                throw new IllegalArgumentException("Unsupported request key: " + key);
            }
        }
    }

    public static String requireMarker(String marker) {
        if (marker == null || !marker.matches("[A-Za-z0-9_-]{1," + MAX_MARKER_LENGTH + "}")) {
            throw new IllegalArgumentException(
                    "Marker must match [A-Za-z0-9_-]{1," + MAX_MARKER_LENGTH + "}"
            );
        }
        return marker;
    }

    public static void requireNoArg(String arg) {
        if (arg != null) {
            throw new IllegalArgumentException("This operation does not accept an arg");
        }
    }

    public static int requireBoundedInt(String name, int value, int minimum, int maximum) {
        if (value < minimum || value > maximum) {
            throw new IllegalArgumentException(
                    name + " must be between " + minimum + " and " + maximum
            );
        }
        return value;
    }

    public static int requireBoundedArg(String name, String value, int minimum, int maximum) {
        final int parsed;
        try {
            parsed = Integer.parseInt(value);
        } catch (NumberFormatException error) {
            throw new IllegalArgumentException(name + " must be an integer", error);
        }
        return requireBoundedInt(name, parsed, minimum, maximum);
    }
}
