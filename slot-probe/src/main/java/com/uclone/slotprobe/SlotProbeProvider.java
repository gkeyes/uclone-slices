package com.uclone.slotprobe;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;

public class SlotProbeProvider extends ContentProvider {
    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public Bundle call(String method, String arg, Bundle extras) {
        Context context = getContext();
        if (context == null) {
            throw new IllegalStateException("Provider is not attached to a context");
        }
        return switch (method) {
            case ProbeContract.METHOD_WRITE -> {
                ProbeContract.requireRequest(extras);
                String marker = ProbeContract.requireMarker(arg);
                yield ProbeOperationLock.call(context, () -> {
                    ProbeStateStore.write(context, marker);
                    return ProbeStateStore.read(context, ProbeContract.METHOD_WRITE);
                });
            }
            case ProbeContract.METHOD_READ -> {
                ProbeContract.requireRequest(extras);
                ProbeContract.requireNoArg(arg);
                yield ProbeStateStore.read(context, ProbeContract.METHOD_READ);
            }
            case ProbeContract.METHOD_HOLD -> {
                ProbeContract.requireRequest(extras);
                holdProcess(arg);
                yield ProbeStateStore.read(context, ProbeContract.METHOD_HOLD);
            }
            case ProbeContract.METHOD_WORKER_START -> fixedRequest(
                    arg,
                    extras,
                    () -> WorkerClient.start(context)
            );
            case ProbeContract.METHOD_WORKER_READ -> fixedRequest(
                    arg,
                    extras,
                    () -> WorkerClient.read(context)
            );
            case ProbeContract.METHOD_WORKER_STOP -> fixedRequest(
                    arg,
                    extras,
                    () -> WorkerClient.stop(context)
            );
            case ProbeContract.METHOD_NATIVE_WRITE -> {
                ProbeContract.requireRequest(extras);
                yield NativeProbe.write(context, ProbeContract.requireMarker(arg));
            }
            case ProbeContract.METHOD_NATIVE_READ -> {
                ProbeContract.requireRequest(extras);
                ProbeContract.requireNoArg(arg);
                yield NativeProbe.read(context, ProbeContract.METHOD_NATIVE_READ);
            }
            case ProbeContract.METHOD_WEBVIEW_WRITE -> {
                ProbeContract.requireRequest(extras);
                yield WebViewProbe.write(context, ProbeContract.requireMarker(arg));
            }
            case ProbeContract.METHOD_WEBVIEW_READ -> {
                ProbeContract.requireRequest(extras);
                ProbeContract.requireNoArg(arg);
                yield WebViewProbe.read(context);
            }
            case ProbeContract.METHOD_WAL_STRESS -> {
                ProbeContract.requireRequest(extras, ProbeContract.EXTRA_ITERATIONS);
                int iterations = extras.getInt(ProbeContract.EXTRA_ITERATIONS, -1);
                yield ProbeDatabase.stress(
                        context,
                        ProbeContract.requireMarker(arg),
                        iterations
                );
            }
            case ProbeContract.METHOD_JOB_SCHEDULE -> scheduleJob(context, arg, extras);
            case ProbeContract.METHOD_JOB_CANCEL -> fixedRequest(
                    arg,
                    extras,
                    () -> ProbeScheduler.cancelJob(context)
            );
            case ProbeContract.METHOD_JOB_READ -> fixedRequest(
                    arg,
                    extras,
                    () -> ProbeScheduler.readJob(context)
            );
            case ProbeContract.METHOD_ALARM_SCHEDULE -> scheduleAlarm(context, arg, extras);
            case ProbeContract.METHOD_ALARM_CANCEL -> fixedRequest(
                    arg,
                    extras,
                    () -> ProbeScheduler.cancelAlarm(context)
            );
            case ProbeContract.METHOD_ALARM_READ -> fixedRequest(
                    arg,
                    extras,
                    () -> ProbeScheduler.readAlarm(context)
            );
            case ProbeContract.METHOD_DIRECT_BOOT_READ -> fixedRequest(
                    arg,
                    extras,
                    () -> ScheduledProbeStore.readDirectBoot(context)
            );
            default -> throw new IllegalArgumentException("Unknown method: " + method);
        };
    }

    private static Bundle fixedRequest(String arg, Bundle extras, ResultSupplier supplier) {
        ProbeContract.requireRequest(extras);
        ProbeContract.requireNoArg(arg);
        return supplier.get();
    }

    private static Bundle scheduleJob(Context context, String marker, Bundle extras) {
        ProbeContract.requireRequest(extras, ProbeContract.EXTRA_DELAY_SECONDS);
        return ProbeScheduler.scheduleJob(
                context,
                ProbeContract.requireMarker(marker),
                extras.getInt(ProbeContract.EXTRA_DELAY_SECONDS, -1)
        );
    }

    private static Bundle scheduleAlarm(Context context, String marker, Bundle extras) {
        ProbeContract.requireRequest(extras, ProbeContract.EXTRA_DELAY_SECONDS);
        return ProbeScheduler.scheduleAlarm(
                context,
                ProbeContract.requireMarker(marker),
                extras.getInt(ProbeContract.EXTRA_DELAY_SECONDS, -1)
        );
    }

    private static void holdProcess(String secondsValue) {
        int seconds = ProbeContract.requireBoundedArg(
                "Hold duration",
                secondsValue,
                1,
                ProbeContract.MAX_HOLD_SECONDS
        );
        try {
            Thread.sleep(seconds * 1_000L);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Hold interrupted", error);
        }
    }

    @FunctionalInterface
    private interface ResultSupplier {
        Bundle get();
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] selectionArgs, String sortOrder) {
        throw new UnsupportedOperationException();
    }

    @Override
    public String getType(Uri uri) {
        return null;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }
}
