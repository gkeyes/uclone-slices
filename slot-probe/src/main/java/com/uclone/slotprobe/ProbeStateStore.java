package com.uclone.slotprobe;

import android.content.Context;
import android.content.SharedPreferences;
import android.os.Bundle;

import java.io.File;

final class ProbeStateStore {
    private static final String MARKER_FILE = "slot-marker.txt";
    private static final String TRANSACTION_FILE = "slot-marker.txt.txn";
    private static final String LEGACY_PREFERENCES_NAME = "slot-probe";
    private static final String PREFERENCES_PREFIX = "slot-probe-txn-";

    private ProbeStateStore() {
    }

    static void write(Context context, String marker) {
        ProbeOperationLock.run(context, () -> {
            String previousTransactionId = ProbeFileIo.read(
                    new File(context.getFilesDir(), TRANSACTION_FILE)
            );
            if (!"<missing>".equals(previousTransactionId)) {
                preferencesName(previousTransactionId);
            }
            String transactionId = ProbeTransaction.begin(
                    context,
                    ProbeTransaction.STATE_SCOPE,
                    marker
            );
            writeUnlocked(context, marker, transactionId);
            verifyUnlocked(context, marker, transactionId);
            ProbeTransaction.commit(
                    context,
                    ProbeTransaction.STATE_SCOPE,
                    transactionId,
                    marker
            );
            retirePreviousPreferences(context, previousTransactionId, transactionId);
        });
    }

    static Bundle read(Context context, String operation) {
        return ProbeOperationLock.call(context, () -> {
            Bundle result = ProbeContract.response(operation);
            addStateUnlocked(context, result, true);
            return result;
        });
    }

    static void addState(Context context, Bundle result) {
        ProbeOperationLock.run(context, () -> addStateUnlocked(context, result, true));
    }

    private static void writeUnlocked(Context context, String marker, String transactionId) {
        ProbeContract.requireMarker(marker);
        ProbeFileIo.replaceSynced(new File(context.getFilesDir(), MARKER_FILE), marker);
        ProbeFileIo.replaceSynced(new File(context.getFilesDir(), TRANSACTION_FILE), transactionId);
        Context deviceContext = context.createDeviceProtectedStorageContext();
        ProbeFileIo.replaceSynced(new File(deviceContext.getFilesDir(), MARKER_FILE), marker);
        ProbeFileIo.replaceSynced(new File(deviceContext.getFilesDir(), TRANSACTION_FILE), transactionId);
        if (!preferences(context, transactionId)
                .edit()
                .putString("marker", marker)
                .putString("transaction_id", transactionId)
                .commit()) {
            throw new IllegalStateException("Failed to commit SharedPreferences marker");
        }
        ProbeDatabase.write(context, marker, transactionId);
    }

    private static void addStateUnlocked(Context context, Bundle result, boolean requireTransaction) {
        Context deviceContext = context.createDeviceProtectedStorageContext();
        ProcessReport.addTo(result, context);
        String ceMarker = ProbeFileIo.read(new File(context.getFilesDir(), MARKER_FILE));
        String deMarker = ProbeFileIo.read(new File(deviceContext.getFilesDir(), MARKER_FILE));
        String ceTransactionId = ProbeFileIo.read(new File(context.getFilesDir(), TRANSACTION_FILE));
        String deTransactionId = ProbeFileIo.read(new File(deviceContext.getFilesDir(), TRANSACTION_FILE));
        SharedPreferences preferences = preferencesForRead(context, ceTransactionId);
        String preferencesMarker = preferences.getString("marker", "<missing>");
        String preferencesTransactionId = preferences.getString("transaction_id", "<missing>");
        result.putString("ceMarker", ceMarker);
        result.putString("deMarker", deMarker);
        result.putString("ceTransactionId", ceTransactionId);
        result.putString("deTransactionId", deTransactionId);
        result.putString("preferencesMarker", preferencesMarker);
        result.putString("preferencesTransactionId", preferencesTransactionId);
        ProbeDatabase.addState(context, result);
        String databaseMarker = result.getString("databaseMarker", "<missing>");
        String databaseTransactionId = result.getString("databaseTransactionId", "<missing>");
        ProbeTransaction.Record transaction = ProbeTransaction.read(
                context,
                ProbeTransaction.STATE_SCOPE
        );
        if (requireTransaction && transaction.present() && !transaction.committed()) {
            throw new IllegalStateException("Probe state transaction is not committed");
        }
        if (allMissing(ceMarker, deMarker, preferencesMarker, databaseMarker)) {
            if (transaction.present()) {
                throw new IllegalStateException("Probe state transaction surfaces are missing");
            }
            return;
        }
        if (!requireTransaction
                || ProbeContract.METHOD_WAL_STRESS.equals(result.getString(ProbeContract.KEY_OPERATION))) {
            return;
        }
        transaction = ProbeTransaction.requireCommitted(
                context,
                ProbeTransaction.STATE_SCOPE,
                ceMarker
        );
        if (!sameId(
                transaction.id(),
                ceTransactionId,
                deTransactionId,
                preferencesTransactionId,
                databaseTransactionId
        ) || !sameMarker(transaction.marker(), deMarker, preferencesMarker, databaseMarker)) {
            throw new IllegalStateException("Probe state surfaces are not from one transaction");
        }
        result.putString("transactionId", transaction.id());
    }

    private static void verifyUnlocked(Context context, String marker, String transactionId) {
        Bundle result = new Bundle();
        addStateUnlocked(context, result, false);
        if (!sameMarker(
                marker,
                result.getString("ceMarker"),
                result.getString("deMarker"),
                result.getString("preferencesMarker"),
                result.getString("databaseMarker")
        ) || !sameId(
                transactionId,
                result.getString("ceTransactionId"),
                result.getString("deTransactionId"),
                result.getString("preferencesTransactionId"),
                result.getString("databaseTransactionId")
        )) {
            throw new IllegalStateException("Probe state transaction verification failed");
        }
    }

    private static boolean allMissing(String... values) {
        for (String value : values) {
            if (!"<missing>".equals(value)) {
                return false;
            }
        }
        return true;
    }

    private static SharedPreferences preferences(Context context, String transactionId) {
        return context.getSharedPreferences(preferencesName(transactionId), Context.MODE_PRIVATE);
    }

    private static SharedPreferences preferencesForRead(Context context, String transactionId) {
        if ("<missing>".equals(transactionId)) {
            return context.getSharedPreferences(LEGACY_PREFERENCES_NAME, Context.MODE_PRIVATE);
        }
        SharedPreferences current = preferences(context, transactionId);
        if (current.contains("transaction_id")) {
            return current;
        }
        SharedPreferences legacy = context.getSharedPreferences(
                LEGACY_PREFERENCES_NAME,
                Context.MODE_PRIVATE
        );
        if (transactionId.equals(legacy.getString("transaction_id", "<missing>"))) {
            return legacy;
        }
        return current;
    }

    private static void retirePreviousPreferences(
            Context context,
            String previousTransactionId,
            String currentTransactionId
    ) {
        if (!"<missing>".equals(previousTransactionId)
                && !previousTransactionId.equals(currentTransactionId)) {
            context.deleteSharedPreferences(preferencesName(previousTransactionId));
        }
        context.deleteSharedPreferences(LEGACY_PREFERENCES_NAME);
    }

    private static String preferencesName(String transactionId) {
        if (transactionId.length() > 96 || !transactionId.matches("[0-9]+-[0-9]+-[0-9]+")) {
            throw new IllegalStateException("Probe transaction id is unsafe for preferences");
        }
        return PREFERENCES_PREFIX + transactionId;
    }

    private static boolean sameMarker(String expected, String... values) {
        for (String value : values) {
            if (!expected.equals(value)) {
                return false;
            }
        }
        return true;
    }

    private static boolean sameId(String expected, String... values) {
        for (String value : values) {
            if (!expected.equals(value)) {
                return false;
            }
        }
        return true;
    }
}
