package com.uclone.slotbridge;

import android.os.Process;
import android.system.ErrnoException;
import android.system.Os;
import android.system.OsConstants;
import android.system.StructStat;
import java.io.FileDescriptor;
import java.io.InterruptedIOException;

final class FixedRestrictionsFile {
    private static final int MAX_BYTES = 4 * 1024 * 1024;
    static final int MAX_ATTEMPTS = 3;
    static final long RETRY_DELAY_MILLIS = 20L;
    private static final int DIRECTORY_FLAGS =
            OsConstants.O_RDONLY | OsConstants.O_CLOEXEC | OsConstants.O_NOFOLLOW;
    private static final int FILE_FLAGS =
            OsConstants.O_RDONLY | OsConstants.O_CLOEXEC | OsConstants.O_NOFOLLOW;
    private static final String SYSTEM_PATH = "/data/system";
    private static final String USERS_PATH = "/data/system/users";
    private static final String USER_ZERO_PATH = "/data/system/users/0";
    private static final String RESTRICTIONS_PATH =
            "/data/system/users/0/package-restrictions.xml";

    private FixedRestrictionsFile() {}

    static byte[] read() throws BridgeFailure {
        return readWithRetry(FixedRestrictionsFile::readOnce, FixedRestrictionsFile::sleep);
    }

    static byte[] readWithRetry(
            RestrictionsFileAttempt attempt, RestrictionsRetrySleeper sleeper)
            throws BridgeFailure {
        if (attempt == null || sleeper == null) {
            throw invalid();
        }
        for (int number = 1; number <= MAX_ATTEMPTS; number++) {
            try {
                return attempt.read();
            } catch (TransientRestrictionsRewrite rewrite) {
                if (number == MAX_ATTEMPTS) {
                    throw invalid();
                }
                try {
                    sleeper.sleep(RETRY_DELAY_MILLIS);
                } catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    throw invalid();
                }
            }
        }
        throw invalid();
    }

    private static byte[] readOnce()
            throws BridgeFailure, TransientRestrictionsRewrite {
        FileDescriptor system = null;
        FileDescriptor users = null;
        FileDescriptor userZero = null;
        FileDescriptor file = null;
        try {
            validateFixedDirectory(SYSTEM_PATH);
            validateFixedDirectory(USERS_PATH);
            validateFixedDirectory(USER_ZERO_PATH);
            system = Os.open(SYSTEM_PATH, DIRECTORY_FLAGS, 0);
            validateDirectory(Os.fstat(system));
            users = Os.open(USERS_PATH, DIRECTORY_FLAGS, 0);
            validateDirectory(Os.fstat(users));
            userZero = Os.open(USER_ZERO_PATH, DIRECTORY_FLAGS, 0);
            validateDirectory(Os.fstat(userZero));
            file = openRestrictionsFile();
            StructStat before = Os.fstat(file);
            validateFile(before);
            byte[] bytes = readExact(file, before.st_size);
            StructStat after = Os.fstat(file);
            if (after.st_ino != before.st_ino || after.st_size != before.st_size) {
                throw new TransientRestrictionsRewrite();
            }
            if (after.st_uid != before.st_uid
                    || after.st_gid != before.st_gid
                    || after.st_mode != before.st_mode
                    || after.st_nlink != before.st_nlink) {
                throw invalid();
            }
            validateFixedDirectory(SYSTEM_PATH);
            validateFixedDirectory(USERS_PATH);
            validateFixedDirectory(USER_ZERO_PATH);
            return bytes;
        } catch (BridgeFailure failure) {
            throw failure;
        } catch (ErrnoException | RuntimeException | LinkageError failure) {
            throw invalid();
        } finally {
            close(file);
            close(userZero);
            close(users);
            close(system);
        }
    }

    private static FileDescriptor openRestrictionsFile()
            throws BridgeFailure, TransientRestrictionsRewrite {
        try {
            return Os.open(RESTRICTIONS_PATH, FILE_FLAGS, 0);
        } catch (ErrnoException failure) {
            if (failure.errno == OsConstants.ENOENT) {
                throw new TransientMissingRestrictionsFile();
            }
            throw invalid();
        }
    }

    private static void sleep(long millis) throws InterruptedException {
        Thread.sleep(millis);
    }

    private static byte[] readExact(FileDescriptor file, long size)
            throws BridgeFailure, TransientRestrictionsRewrite {
        byte[] bytes = new byte[(int) size];
        int offset = 0;
        try {
            while (offset < bytes.length) {
                int count = Os.read(file, bytes, offset, bytes.length - offset);
                if (count <= 0) {
                    throw invalid();
                }
                offset += count;
            }
            byte[] extra = new byte[1];
            if (Os.read(file, extra, 0, 1) != 0) {
                throw new TransientRestrictionsRewrite();
            }
            return bytes;
        } catch (ErrnoException | InterruptedIOException failure) {
            throw invalid();
        }
    }

    private static void validateDirectory(StructStat stat) throws BridgeFailure {
        if ((stat.st_mode & OsConstants.S_IFMT) != OsConstants.S_IFDIR
                || !trustedDirectoryOwner(stat.st_uid, stat.st_gid)
                || (stat.st_mode & 0x2) != 0) {
            throw invalid();
        }
    }

    private static void validateFixedDirectory(String path) throws BridgeFailure {
        try {
            validateDirectory(Os.lstat(path));
        } catch (ErrnoException | RuntimeException | LinkageError failure) {
            throw invalid();
        }
    }

    private static void validateFile(StructStat stat) throws BridgeFailure {
        if ((stat.st_mode & OsConstants.S_IFMT) != OsConstants.S_IFREG
                || stat.st_uid != Process.SYSTEM_UID
                || stat.st_gid != Process.SYSTEM_UID
                || (stat.st_mode & 0x2) != 0
                || stat.st_nlink != 1
                || stat.st_size <= 0
                || stat.st_size > MAX_BYTES) {
            throw invalid();
        }
    }

    private static boolean trustedDirectoryOwner(int uid, int gid) {
        return (uid == 0 || uid == Process.SYSTEM_UID)
                && (gid == 0 || gid == Process.SYSTEM_UID);
    }

    private static void close(FileDescriptor descriptor) {
        if (descriptor == null) {
            return;
        }
        try {
            Os.close(descriptor);
        } catch (ErrnoException ignored) {
            return;
        }
    }

    private static BridgeFailure invalid() {
        return new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
    }
}

@FunctionalInterface
interface RestrictionsFileAttempt {
    byte[] read() throws BridgeFailure, TransientRestrictionsRewrite;
}

@FunctionalInterface
interface RestrictionsRetrySleeper {
    void sleep(long millis) throws InterruptedException;
}

class TransientRestrictionsRewrite extends Exception {
    private static final long serialVersionUID = 1L;
}

final class TransientMissingRestrictionsFile extends TransientRestrictionsRewrite {
    private static final long serialVersionUID = 1L;
}
