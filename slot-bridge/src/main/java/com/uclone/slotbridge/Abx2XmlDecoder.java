package com.uclone.slotbridge;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

final class Abx2XmlDecoder implements AbxDecoderRunner {
    static final int MAX_INPUT_BYTES = 4 * 1024 * 1024;
    static final int MAX_OUTPUT_BYTES = 4 * 1024 * 1024;
    static final long TIMEOUT_MILLIS = 2_000L;
    static final long CLEANUP_WAIT_MILLIS = 250L;
    static final int MAX_BOOTCLASSPATH_BYTES = AbxProcessEnvironment.MAX_BOOTCLASSPATH_BYTES;

    private static final String ABX2XML_PATH = "/system/bin/abx2xml";
    private static final String[] COMMAND = {
        "/system/bin/app_process",
        "/system/bin",
        "com.android.commands.abx.Abx",
        ABX2XML_PATH,
        "-",
        "-"
    };
    private static final ThreadFactory THREAD_FACTORY =
            runnable -> {
                Thread thread = new Thread(runnable, "uclone-abx-decoder");
                thread.setDaemon(true);
                return thread;
            };

    private final AbxProcessLauncher processLauncher;
    private final long timeoutMillis;

    Abx2XmlDecoder() {
        this(Abx2XmlDecoder::startProcess, TIMEOUT_MILLIS);
    }

    Abx2XmlDecoder(AbxProcessLauncher processLauncher, long timeoutMillis) {
        if (processLauncher == null || timeoutMillis <= 0) {
            throw new IllegalArgumentException();
        }
        this.processLauncher = processLauncher;
        this.timeoutMillis = timeoutMillis;
    }

    @Override
    public byte[] decode(byte[] input) throws BridgeFailure {
        if (input == null || input.length == 0 || input.length > MAX_INPUT_BYTES) {
            throw invalid();
        }
        Process process = null;
        ExecutorService workers = Executors.newFixedThreadPool(3, THREAD_FACTORY);
        long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMillis);
        boolean completed = false;
        try {
            process = processLauncher.start();
            Process running = process;
            Future<byte[]> output = workers.submit(() -> readBounded(running.getInputStream()));
            Future<?> errors = workers.submit(
                    () -> {
                        drain(running.getErrorStream());
                        return null;
                    });
            Future<?> inputWriter = workers.submit(
                    () -> {
                        writeInput(running.getOutputStream(), input);
                        return null;
                    });
            if (!running.waitFor(remainingMillis(deadline), TimeUnit.MILLISECONDS)) {
                throw invalid();
            }
            inputWriter.get(remainingMillis(deadline), TimeUnit.MILLISECONDS);
            errors.get(remainingMillis(deadline), TimeUnit.MILLISECONDS);
            byte[] decoded = output.get(remainingMillis(deadline), TimeUnit.MILLISECONDS);
            if (running.exitValue() != 0 || decoded.length == 0) {
                throw invalid();
            }
            completed = true;
            return decoded;
        } catch (BridgeFailure failure) {
            throw failure;
        } catch (InterruptedException failure) {
            Thread.currentThread().interrupt();
            throw invalid();
        } catch (IOException | ExecutionException | TimeoutException | RuntimeException | LinkageError failure) {
            throw invalid();
        } finally {
            cleanupProcess(process, !completed);
            workers.shutdownNow();
            try {
                workers.awaitTermination(100, TimeUnit.MILLISECONDS);
            } catch (InterruptedException failure) {
                Thread.currentThread().interrupt();
            }
        }
    }

    private static Process startProcess() throws IOException {
        ProcessBuilder builder = new ProcessBuilder(COMMAND.clone());
        Map<String, String> environment = builder.environment();
        try {
            AbxProcessEnvironment.populate(environment, System.getenv());
        } catch (BridgeFailure failure) {
            throw new IOException(failure);
        }
        return builder.start();
    }

    static void populateEnvironment(Map<String, String> environment, Map<String, String> source)
            throws BridgeFailure {
        AbxProcessEnvironment.populate(environment, source);
    }

    private static void writeInput(OutputStream stream, byte[] input) throws IOException {
        try (OutputStream output = stream) {
            output.write(input);
        }
    }

    private static void drain(InputStream stream) throws IOException {
        try (InputStream input = stream) {
            byte[] buffer = new byte[8192];
            while (input.read(buffer) >= 0) {
            }
        }
    }

    private static byte[] readBounded(InputStream stream) throws IOException {
        try (InputStream input = stream) {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[8192];
            while (true) {
                int count = input.read(buffer);
                if (count < 0) {
                    return output.toByteArray();
                }
                if (count == 0) {
                    continue;
                }
                if (count > MAX_OUTPUT_BYTES - output.size()) {
                    throw new IOException("abx output limit");
                }
                output.write(buffer, 0, count);
            }
        }
    }

    private static long remainingMillis(long deadline) {
        long nanos = deadline - System.nanoTime();
        return Math.max(1L, TimeUnit.NANOSECONDS.toMillis(nanos));
    }

    private static void cleanupProcess(Process process, boolean force) {
        if (process == null) {
            return;
        }
        if (force || isAlive(process)) {
            force(process);
        }
        reap(process, CLEANUP_WAIT_MILLIS);
        if (isAlive(process)) {
            force(process);
            reap(process, CLEANUP_WAIT_MILLIS);
        }
        close(process.getOutputStream());
        close(process.getInputStream());
        close(process.getErrorStream());
    }

    private static void force(Process process) {
        try {
            process.destroyForcibly();
        } catch (RuntimeException ignored) {
            return;
        }
    }

    private static boolean isAlive(Process process) {
        try {
            return process.isAlive();
        } catch (RuntimeException ignored) {
            return false;
        }
    }

    private static void reap(Process process, long timeoutMillis) {
        try {
            process.waitFor(timeoutMillis, TimeUnit.MILLISECONDS);
        } catch (InterruptedException failure) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException ignored) {
            return;
        }
    }

    private static void close(java.io.Closeable stream) {
        try {
            stream.close();
        } catch (IOException | RuntimeException ignored) {
            return;
        }
    }

    private static BridgeFailure invalid() {
        return new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
    }
}
