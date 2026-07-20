package com.uclone.slotprobe;

import android.content.Context;

import java.io.File;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.util.concurrent.locks.ReentrantLock;

final class ProbeOperationLock {
    private static final String LOCK_FILE = "probe-operation.lock";
    private static final ReentrantLock PROCESS_LOCK = new ReentrantLock(true);
    private static final ThreadLocal<Integer> DEPTH = ThreadLocal.withInitial(() -> 0);

    private ProbeOperationLock() {
    }

    static <T> T call(Context context, Operation<T> operation) {
        int depth = DEPTH.get();
        if (depth > 0) {
            DEPTH.set(depth + 1);
            try {
                return operation.run();
            } finally {
                DEPTH.set(depth);
            }
        }
        PROCESS_LOCK.lock();
        try {
            return withFileLock(context, operation);
        } finally {
            PROCESS_LOCK.unlock();
        }
    }

    static void run(Context context, VoidOperation operation) {
        call(context, () -> {
            operation.run();
            return null;
        });
    }

    private static <T> T withFileLock(Context context, Operation<T> operation) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        File lockFile = new File(deviceContext.getFilesDir(), LOCK_FILE);
        try (RandomAccessFile file = new RandomAccessFile(lockFile, "rw");
             FileChannel channel = file.getChannel();
             FileLock ignored = channel.lock()) {
            DEPTH.set(1);
            try {
                return operation.run();
            } finally {
                DEPTH.remove();
            }
        } catch (IOException error) {
            throw new IllegalStateException("Failed to acquire probe operation lock", error);
        }
    }

    @FunctionalInterface
    interface Operation<T> {
        T run();
    }

    @FunctionalInterface
    interface VoidOperation {
        void run();
    }
}
