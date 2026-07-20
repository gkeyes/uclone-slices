package com.uclone.slotprobe;

import android.content.Context;
import android.database.Cursor;
import android.database.sqlite.SQLiteDatabase;
import android.os.Bundle;

final class ProbeDatabase {
    private static final String DATABASE_NAME = "slot-probe.db";
    private static final String CREATE_SQL = "CREATE TABLE IF NOT EXISTS probe_state "
            + "(id INTEGER PRIMARY KEY, marker TEXT NOT NULL, generation INTEGER NOT NULL, "
            + "transaction_id TEXT NOT NULL DEFAULT '')";
    private static final String UPSERT_SQL = "INSERT INTO probe_state(id, marker, generation, transaction_id) "
            + "VALUES(1, ?, 1, ?) ON CONFLICT(id) DO UPDATE SET "
            + "marker=excluded.marker, generation=probe_state.generation+1, "
            + "transaction_id=excluded.transaction_id";

    private ProbeDatabase() {
    }

    static void write(Context context, String marker, String transactionId) {
        try (SQLiteDatabase database = open(context)) {
            upsert(database, marker, transactionId);
        }
    }

    static Bundle stress(Context context, String marker, int iterations) {
        return ProbeOperationLock.call(context, () -> stressUnlocked(context, marker, iterations));
    }

    private static Bundle stressUnlocked(Context context, String marker, int iterations) {
        ProbeContract.requireBoundedInt(
                ProbeContract.EXTRA_ITERATIONS,
                iterations,
                1,
                ProbeContract.MAX_STRESS_ITERATIONS
        );
        String journalMode;
        try (SQLiteDatabase database = open(context)) {
            journalMode = journalMode(database);
            for (int index = 0; index < iterations; index++) {
                upsert(database, marker, "wal-stress-" + index);
            }
        }
        Bundle result = ProbeContract.response(ProbeContract.METHOD_WAL_STRESS);
        result.putInt(ProbeContract.EXTRA_ITERATIONS, iterations);
        result.putBoolean("walEnabled", "wal".equalsIgnoreCase(journalMode));
        result.putString("journalMode", journalMode);
        addState(context, result);
        return result;
    }

    static void addState(Context context, Bundle result) {
        try (SQLiteDatabase database = open(context);
             Cursor cursor = database.rawQuery(
                     "SELECT marker, generation, transaction_id FROM probe_state WHERE id=1",
                     null
             )) {
            if (cursor.moveToFirst()) {
                result.putString("databaseMarker", cursor.getString(0));
                result.putLong("databaseGeneration", cursor.getLong(1));
                result.putString("databaseTransactionId", cursor.getString(2));
            } else {
                result.putString("databaseMarker", "<missing>");
                result.putLong("databaseGeneration", 0);
                result.putString("databaseTransactionId", "<missing>");
            }
        }
    }

    private static SQLiteDatabase open(Context context) {
        SQLiteDatabase database = context.openOrCreateDatabase(
                DATABASE_NAME,
                Context.MODE_PRIVATE,
                null
        );
        database.enableWriteAheadLogging();
        if (!database.isWriteAheadLoggingEnabled()) {
            database.close();
            throw new IllegalStateException("SQLite WAL could not be enabled");
        }
        journalMode(database);
        database.execSQL(CREATE_SQL);
        ensureTransactionColumn(database);
        return database;
    }

    private static String journalMode(SQLiteDatabase database) {
        try (Cursor cursor = database.rawQuery("PRAGMA journal_mode", null)) {
            if (!cursor.moveToFirst()) {
                throw new IllegalStateException("SQLite did not report journal_mode");
            }
            String mode = cursor.getString(0);
            if (!"wal".equalsIgnoreCase(mode)) {
                throw new IllegalStateException("SQLite journal_mode is not WAL: " + mode);
            }
            return mode;
        }
    }

    private static void upsert(SQLiteDatabase database, String marker, String transactionId) {
        database.execSQL(UPSERT_SQL, new Object[]{marker, transactionId});
    }

    private static void ensureTransactionColumn(SQLiteDatabase database) {
        try (Cursor cursor = database.rawQuery("PRAGMA table_info(probe_state)", null)) {
            while (cursor.moveToNext()) {
                if ("transaction_id".equals(cursor.getString(1))) {
                    return;
                }
            }
        }
        database.execSQL("ALTER TABLE probe_state ADD COLUMN transaction_id TEXT NOT NULL DEFAULT ''");
    }
}
