package com.uclone.slotprobe;

import android.content.Context;
import android.os.Bundle;

import java.io.File;

final class NativeProbe {
    private static final String DIRECTORY_NAME = "native-probe";
    private static final String TRANSACTION_FILE = "native-slot-marker.txt.txn";

    static {
        System.loadLibrary("slotprobe_native");
    }

    private NativeProbe() {
    }

    static Bundle write(Context context, String marker) {
        ProbeContract.requireMarker(marker);
        return ProbeOperationLock.call(context, () -> {
            String transactionId = ProbeTransaction.begin(
                    context,
                    ProbeTransaction.NATIVE_SCOPE,
                    marker
            );
            Directories directories = prepareDirectories(context);
            nativeWrite(directories.cePath, directories.dePath, marker);
            ProbeFileIo.replaceSynced(
                    new File(directories.cePath, TRANSACTION_FILE),
                    transactionId
            );
            ProbeFileIo.replaceSynced(
                    new File(directories.dePath, TRANSACTION_FILE),
                    transactionId
            );
            String[] markers = readMarkers(directories);
            requireMarkers(markers, marker);
            ProbeTransaction.commit(
                    context,
                    ProbeTransaction.NATIVE_SCOPE,
                    transactionId,
                    marker
            );
            return readUnlocked(context, ProbeContract.METHOD_NATIVE_WRITE, markers);
        });
    }

    static Bundle read(Context context, String operation) {
        return ProbeOperationLock.call(context, () -> readUnlocked(
                context,
                operation,
                readMarkers(prepareDirectories(context))
        ));
    }

    private static Bundle readUnlocked(Context context, String operation, String[] markers) {
        requireMarkers(markers, markers[0]);
        ProbeTransaction.Record transaction = ProbeTransaction.requireCommitted(
                context,
                ProbeTransaction.NATIVE_SCOPE,
                markers[0]
        );
        File ceDirectory = new File(context.getFilesDir(), DIRECTORY_NAME);
        File deDirectory = new File(
                context.createDeviceProtectedStorageContext().getFilesDir(),
                DIRECTORY_NAME
        );
        if (!transaction.id().equals(ProbeFileIo.read(new File(ceDirectory, TRANSACTION_FILE)))
                || !transaction.id().equals(ProbeFileIo.read(new File(deDirectory, TRANSACTION_FILE)))) {
            throw new IllegalStateException("Native transaction surfaces are not coherent");
        }
        Bundle result = ProbeContract.response(operation);
        ProcessReport.addTo(result, context);
        result.putBoolean("nativeAvailable", true);
        result.putString("nativeTransactionId", transaction.id());
        result.putString("nativeCeMarker", markers[0]);
        result.putString("nativeDeMarker", markers[1]);
        result.putString("nativeMmapMarker", markers[2]);
        return result;
    }

    private static String[] readMarkers(Directories directories) {
        String[] markers = nativeRead(directories.cePath, directories.dePath);
        if (markers == null || markers.length != 3) {
            throw new IllegalStateException("Native probe returned an invalid response");
        }
        return markers;
    }

    private static void requireMarkers(String[] markers, String expected) {
        if (markers == null || markers.length != 3) {
            throw new IllegalStateException("Native probe returned an invalid response");
        }
        for (String marker : markers) {
            try {
                ProbeContract.requireMarker(marker);
            } catch (IllegalArgumentException error) {
                throw new IllegalStateException("Native probe returned an invalid marker", error);
            }
            if (!expected.equals(marker)) {
                throw new IllegalStateException("Native probe surfaces are not coherent");
            }
        }
    }

    private static Directories prepareDirectories(Context context) {
        File ceDirectory = new File(context.getFilesDir(), DIRECTORY_NAME);
        File deDirectory = new File(
                context.createDeviceProtectedStorageContext().getFilesDir(),
                DIRECTORY_NAME
        );
        requireDirectory(ceDirectory);
        requireDirectory(deDirectory);
        return new Directories(
                ProbeFileIo.canonicalPath(ceDirectory),
                ProbeFileIo.canonicalPath(deDirectory)
        );
    }

    private static void requireDirectory(File directory) {
        if (!directory.isDirectory() && !directory.mkdirs()) {
            throw new IllegalStateException("Failed to create native probe directory");
        }
    }

    private static native void nativeWrite(String ceDirectory, String deDirectory, String marker);

    private static native String[] nativeRead(String ceDirectory, String deDirectory);

    private record Directories(String cePath, String dePath) {
    }
}
