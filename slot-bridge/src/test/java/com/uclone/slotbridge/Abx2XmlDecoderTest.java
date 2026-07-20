package com.uclone.slotbridge;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import java.io.InputStream;
import java.io.OutputStream;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.Test;

public final class Abx2XmlDecoderTest {
    @Test
    public void passesValidatedBootClasspathsIntoFixedEnvironment() throws BridgeFailure {
        Map<String, String> source = new HashMap<>();
        source.put(
                "BOOTCLASSPATH",
                "/system/framework/core-oj.jar:/apex/com.android.art/javalib/core-libart.jar");
        source.put("DEX2OATBOOTCLASSPATH", "/system_ext/framework/ext.jar");
        Map<String, String> environment = new HashMap<>();
        environment.put("UNCONTROLLED", "/data/should-not-pass");

        Abx2XmlDecoder.populateEnvironment(environment, source);

        assertEquals(source.get("BOOTCLASSPATH"), environment.get("BOOTCLASSPATH"));
        assertEquals(source.get("DEX2OATBOOTCLASSPATH"), environment.get("DEX2OATBOOTCLASSPATH"));
        assertFalse(environment.containsKey("UNCONTROLLED"));
        assertEquals("/system", environment.get("ANDROID_ROOT"));
    }

    @Test
    public void rejectsMissingBootClasspath() {
        Map<String, String> source = new HashMap<>();
        source.put("BOOTCLASSPATH", "/system/framework/core-oj.jar");
        assertInvalidEnvironment(source);
    }

    @Test
    public void rejectsUntrustedOrMalformedBootClasspath() {
        String[] malicious = {
            "/data/system.jar",
            "relative.jar",
            "/system/framework/../data.jar",
            "/system/framework/a.jar::/system/framework/b.jar",
            "/system/framework/a\u0000.jar"
        };
        for (String value : malicious) {
            Map<String, String> source = new HashMap<>();
            source.put("BOOTCLASSPATH", value);
            source.put("DEX2OATBOOTCLASSPATH", "/system/framework/core-oj.jar");
            assertInvalidEnvironment(source);
        }
        Map<String, String> oversized = new HashMap<>();
        oversized.put("BOOTCLASSPATH", repeat('a', 65 * 1024));
        oversized.put("DEX2OATBOOTCLASSPATH", "/system/framework/core-oj.jar");
        assertInvalidEnvironment(oversized);
    }

    @Test
    public void timeoutKillsAndReapsRealChildWithoutShell() throws Exception {
        AtomicReference<TrackingProcess> launched = new AtomicReference<>();
        Abx2XmlDecoder decoder = new Abx2XmlDecoder(
                () -> {
                    Process child = new ProcessBuilder(
                                    javaExecutable(),
                                    "-cp",
                                    System.getProperty("java.class.path"),
                                    Abx2XmlDecoderTest.class.getName(),
                                    "sleep")
                            .start();
                    TrackingProcess tracked = new TrackingProcess(child);
                    launched.set(tracked);
                    return tracked;
                },
                100L);

        try {
            decoder.decode(new byte[] {'A', 'B', 'X', 0});
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertTrue(failure.code() == ErrorCode.INVALID_RESPONSE);
        }

        TrackingProcess process = launched.get();
        assertNotNull(process);
        int forceKills = process.forceKills;
        int cleanupWaits = process.cleanupWaits;
        assertTrue(forceKills > 0);
        assertTrue(cleanupWaits > 0);
        assertTrue(process.waitFor(1, TimeUnit.SECONDS));
        assertFalse(process.isAlive());
    }

    public static void main(String[] args) throws Exception {
        if (args.length == 1 && "sleep".equals(args[0])) {
            Thread.sleep(30_000L);
        }
    }

    private static String javaExecutable() {
        return System.getProperty("java.home") + "/bin/java";
    }

    private static void assertInvalidEnvironment(Map<String, String> source) {
        try {
            Abx2XmlDecoder.populateEnvironment(new HashMap<>(), source);
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertTrue(failure.code() == ErrorCode.INVALID_RESPONSE);
        }
    }

    private static String repeat(char value, int count) {
        char[] values = new char[count];
        java.util.Arrays.fill(values, value);
        return new String(values);
    }

    private static final class TrackingProcess extends Process {
        private final Process delegate;
        private int forceKills;
        private int cleanupWaits;

        TrackingProcess(Process delegate) {
            this.delegate = delegate;
        }

        @Override
        public OutputStream getOutputStream() {
            return delegate.getOutputStream();
        }

        @Override
        public InputStream getInputStream() {
            return delegate.getInputStream();
        }

        @Override
        public InputStream getErrorStream() {
            return delegate.getErrorStream();
        }

        @Override
        public int waitFor() throws InterruptedException {
            return delegate.waitFor();
        }

        @Override
        public boolean waitFor(long timeout, TimeUnit unit) throws InterruptedException {
            if (unit.toMillis(timeout) >= Abx2XmlDecoder.CLEANUP_WAIT_MILLIS) {
                cleanupWaits++;
            }
            return delegate.waitFor(timeout, unit);
        }

        @Override
        public int exitValue() {
            return delegate.exitValue();
        }

        @Override
        public void destroy() {
            delegate.destroy();
        }

        @Override
        public Process destroyForcibly() {
            forceKills++;
            delegate.destroyForcibly();
            return this;
        }

        @Override
        public boolean isAlive() {
            return delegate.isAlive();
        }
    }
}
