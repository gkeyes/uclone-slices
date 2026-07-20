package com.uclone.slotbridge;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.fail;

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.Test;

public final class FixedRestrictionsFileTest {
    @Test
    public void retriesOnlyTransientMissingFileAndReturnsStableBytes() throws BridgeFailure {
        byte[] expected = new byte[] {1, 2, 3};
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger sleeps = new AtomicInteger();

        byte[] actual = FixedRestrictionsFile.readWithRetry(
                () -> {
                    if (attempts.getAndIncrement() < 2) {
                        throw new TransientMissingRestrictionsFile();
                    }
                    return expected;
                },
                millis -> sleeps.incrementAndGet());

        assertArrayEquals(expected, actual);
        assertEquals(3, attempts.get());
        assertEquals(2, sleeps.get());
    }

    @Test
    public void stopsAfterBoundedMissingFileAttempts() {
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger sleeps = new AtomicInteger();
        try {
            FixedRestrictionsFile.readWithRetry(
                    () -> {
                        attempts.incrementAndGet();
                        throw new TransientMissingRestrictionsFile();
                    },
                    millis -> sleeps.incrementAndGet());
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
            assertEquals(FixedRestrictionsFile.MAX_ATTEMPTS, attempts.get());
            assertEquals(FixedRestrictionsFile.MAX_ATTEMPTS - 1, sleeps.get());
        }
    }

    @Test
    public void doesNotRetryPermanentValidationFailure() {
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger sleeps = new AtomicInteger();
        try {
            FixedRestrictionsFile.readWithRetry(
                    () -> {
                        attempts.incrementAndGet();
                        throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
                    },
                    millis -> sleeps.incrementAndGet());
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
            assertEquals(1, attempts.get());
            assertEquals(0, sleeps.get());
        }
    }

    @Test
    public void retriesGrowingReadAndMetadataMismatchAsTransientRewrite() throws BridgeFailure {
        byte[] expected = new byte[] {4, 5, 6};
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger sleeps = new AtomicInteger();

        byte[] actual = FixedRestrictionsFile.readWithRetry(
                () -> {
                    int number = attempts.getAndIncrement();
                    if (number < 2) {
                        throw new TransientRestrictionsRewrite();
                    }
                    return expected;
                },
                millis -> sleeps.incrementAndGet());

        assertArrayEquals(expected, actual);
        assertEquals(3, attempts.get());
        assertEquals(2, sleeps.get());
    }

    @Test
    public void stopsAfterBoundedTransientRewriteAttempts() {
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger sleeps = new AtomicInteger();
        try {
            FixedRestrictionsFile.readWithRetry(
                    () -> {
                        attempts.incrementAndGet();
                        throw new TransientRestrictionsRewrite();
                    },
                    millis -> sleeps.incrementAndGet());
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
            assertEquals(FixedRestrictionsFile.MAX_ATTEMPTS, attempts.get());
            assertEquals(FixedRestrictionsFile.MAX_ATTEMPTS - 1, sleeps.get());
        }
    }
}
