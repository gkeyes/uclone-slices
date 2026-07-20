package com.uclone.slotprobe;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.os.Bundle;
import android.os.IBinder;
import android.os.RemoteException;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

final class WorkerClient {
    private static final long BIND_TIMEOUT_MILLIS = 2_000;
    private static final Object LOCK = new Object();
    private static Connection connection;

    private WorkerClient() {
    }

    static Bundle start(Context context) {
        synchronized (LOCK) {
            for (int attempt = 0; attempt < 2; attempt++) {
                try {
                    return ensureConnection(context).readIdentity(ProbeContract.METHOD_WORKER_START);
                } catch (RuntimeException error) {
                    discardConnection(context, connection);
                    if (attempt == 1) {
                        throw error;
                    }
                }
            }
            throw new IllegalStateException("Worker process could not be started");
        }
    }

    static Bundle read(Context context) {
        synchronized (LOCK) {
            for (int attempt = 0; attempt < 2; attempt++) {
                try {
                    return ensureConnection(context).readIdentity(ProbeContract.METHOD_WORKER_READ);
                } catch (RuntimeException error) {
                    discardConnection(context, connection);
                    if (attempt == 1) {
                        throw error;
                    }
                }
            }
            throw new IllegalStateException("Worker process could not be read");
        }
    }

    static Bundle read() {
        synchronized (LOCK) {
            Bundle result = ProbeContract.response(ProbeContract.METHOD_WORKER_READ);
            result.putBoolean("workerBound", connection != null && connection.isAlive());
            return result;
        }
    }

    static Bundle stop(Context context) {
        synchronized (LOCK) {
            discardConnection(context, connection);
            Bundle result = ProbeContract.response(ProbeContract.METHOD_WORKER_STOP);
            result.putBoolean("workerBound", false);
            return result;
        }
    }

    private static Connection ensureConnection(Context context) {
        if (connection != null && connection.isAlive()) {
            return connection;
        }
        discardConnection(context, connection);
        Connection candidate = new Connection();
        Intent intent = new Intent(context, WorkerProbeService.class);
        if (!context.bindService(intent, candidate, Context.BIND_AUTO_CREATE)) {
            throw new IllegalStateException("Failed to bind worker process");
        }
        candidate.markBoundRequested();
        connection = candidate;
        try {
            candidate.awaitBound();
            return candidate;
        } catch (RuntimeException error) {
            discardConnection(context, candidate);
            throw error;
        }
    }

    private static void discardConnection(Context context, Connection current) {
        if (current == null) {
            return;
        }
        current.close();
        if (current.boundRequested()) {
            try {
                context.unbindService(current);
                current.markUnbound();
            } catch (IllegalArgumentException ignored) {
                current.markUnbound();
            }
        }
        if (connection == current) {
            connection = null;
        }
    }

    private static final class Connection implements ServiceConnection {
        private final CountDownLatch bound = new CountDownLatch(1);
        private final AtomicBoolean closed = new AtomicBoolean();
        private final AtomicBoolean boundRequested = new AtomicBoolean();
        private final IBinder.DeathRecipient deathRecipient = this::onBinderDied;
        private volatile IWorkerProbe worker;
        private volatile IBinder binder;

        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            if (service == null || closed.get()) {
                clearWorker();
                bound.countDown();
                return;
            }
            try {
                service.linkToDeath(deathRecipient, 0);
                binder = service;
                worker = IWorkerProbe.Stub.asInterface(service);
            } catch (RemoteException error) {
                clearWorker();
            } finally {
                bound.countDown();
            }
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            clearWorker();
            bound.countDown();
        }

        @Override
        public void onBindingDied(ComponentName name) {
            clearWorker();
            bound.countDown();
        }

        @Override
        public void onNullBinding(ComponentName name) {
            clearWorker();
            bound.countDown();
        }

        void awaitBound() {
            try {
                if (!bound.await(BIND_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS) || !isAlive()) {
                    throw new IllegalStateException("Timed out waiting for worker process");
                }
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("Worker bind interrupted", error);
            }
        }

        Bundle readIdentity(String operation) {
            IWorkerProbe current = worker;
            if (current == null || closed.get()) {
                throw new IllegalStateException("Worker process is not connected");
            }
            try {
                Bundle result = current.readIdentity();
                if (result == null) {
                    throw new IllegalStateException("Worker process returned no response");
                }
                result.putString(ProbeContract.KEY_OPERATION, operation);
                return result;
            } catch (RemoteException error) {
                onBinderDied();
                throw new IllegalStateException("Worker process did not respond", error);
            }
        }

        boolean isAlive() {
            return !closed.get() && worker != null && binder != null;
        }

        void close() {
            if (!closed.compareAndSet(false, true)) {
                return;
            }
            IBinder current = binder;
            clearWorker();
            if (current != null) {
                try {
                    current.unlinkToDeath(deathRecipient, 0);
                } catch (RuntimeException ignored) {
                    clearWorker();
                }
            }
        }

        boolean boundRequested() {
            return boundRequested.get();
        }

        void markBoundRequested() {
            boundRequested.set(true);
        }

        void markUnbound() {
            boundRequested.set(false);
        }

        private void onBinderDied() {
            clearWorker();
            bound.countDown();
        }

        private void clearWorker() {
            worker = null;
            binder = null;
        }
    }
}
