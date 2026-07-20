package com.uclone.slotprobe;

import android.content.Context;
import android.os.Process;

import java.io.File;
import java.util.concurrent.atomic.AtomicLong;

final class ProbeTransaction {
    static final String STATE_SCOPE = "state";
    static final String NATIVE_SCOPE = "native";
    private static final String DIRECTORY_NAME = "probe-transactions";
    private static final AtomicLong SEQUENCE = new AtomicLong();

    private ProbeTransaction() {
    }

    static String begin(Context context, String scope, String marker) {
        requireScope(scope);
        ProbeContract.requireMarker(marker);
        String id = Process.myPid() + "-" + System.nanoTime() + "-" + SEQUENCE.incrementAndGet();
        write(context, scope, new Record(id, marker, false, true));
        return id;
    }

    static void commit(Context context, String scope, String id, String marker) {
        requireScope(scope);
        ProbeContract.requireMarker(marker);
        Record current = read(context, scope);
        if (!current.present() || current.committed() || !id.equals(current.id())
                || !marker.equals(current.marker())) {
            throw new IllegalStateException("Probe transaction commit does not match PREPARED state");
        }
        write(context, scope, new Record(id, marker, true, true));
    }

    static Record read(Context context, String scope) {
        requireScope(scope);
        String value = ProbeFileIo.read(file(context, scope));
        if ("<missing>".equals(value)) {
            return new Record("", "<missing>", false, false);
        }
        String[] fields = value.split("\\|", -1);
        if (fields.length != 4 || (!"PREPARED".equals(fields[0]) && !"COMMITTED".equals(fields[0]))) {
            throw new IllegalStateException("Probe transaction journal is malformed");
        }
        ProbeContract.requireMarker(fields[2]);
        if (fields[1].isEmpty()) {
            throw new IllegalStateException("Probe transaction id is missing");
        }
        if (!"true".equals(fields[3]) && !"false".equals(fields[3])) {
            throw new IllegalStateException("Probe transaction journal flag is malformed");
        }
        return new Record(
                fields[1],
                fields[2],
                "COMMITTED".equals(fields[0]),
                Boolean.parseBoolean(fields[3])
        );
    }

    static Record requireCommitted(Context context, String scope, String marker) {
        Record record = read(context, scope);
        if (!record.present() || !record.committed()) {
            throw new IllegalStateException("Probe transaction is not committed: " + scope);
        }
        if (!record.marker().equals(marker)) {
            throw new IllegalStateException("Probe transaction marker does not match: " + scope);
        }
        return record;
    }

    private static void write(Context context, String scope, Record record) {
        ProbeFileIo.replaceSynced(
                file(context, scope),
                (record.committed() ? "COMMITTED" : "PREPARED") + "|"
                        + record.id() + "|" + record.marker() + "|" + record.present()
        );
    }

    private static File file(Context context, String scope) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        return new File(
                new File(deviceContext.getFilesDir(), DIRECTORY_NAME),
                requireScope(scope) + ".txn"
        );
    }

    private static String requireScope(String scope) {
        if (!STATE_SCOPE.equals(scope) && !NATIVE_SCOPE.equals(scope)) {
            throw new IllegalArgumentException("Unsupported probe transaction scope");
        }
        return scope;
    }

    record Record(String id, String marker, boolean committed, boolean present) {
    }
}
