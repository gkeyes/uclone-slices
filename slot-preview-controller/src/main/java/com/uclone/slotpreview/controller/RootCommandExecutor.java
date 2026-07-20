package com.uclone.slotpreview.controller;

import java.io.IOException;
import java.util.Objects;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

final class RootCommandExecutor {
    private static final long STREAM_TIMEOUT_MILLIS = 2_000L;
    private final ProcessStarter processStarter;

    RootCommandExecutor() {
        this(RootCommandExecutor::startProcess);
    }

    RootCommandExecutor(ProcessStarter processStarter) {
        this.processStarter = Objects.requireNonNull(processStarter, "processStarter");
    }

    CommandResult execute(PreviewCommand.Action action) throws IOException {
        String command = PreviewCommand.build(action);
        Process process = processStarter.start(command);
        ExecutorService readers = Executors.newFixedThreadPool(2);
        Future<BoundedOutput> stdout = readers.submit(
                () -> BoundedOutput.read(process.getInputStream(), BoundedOutput.MAX_BYTES)
        );
        Future<BoundedOutput> stderr = readers.submit(
                () -> BoundedOutput.read(process.getErrorStream(), BoundedOutput.MAX_BYTES)
        );
        boolean timedOut = false;
        boolean interrupted = false;
        int exitCode;
        try {
            if (!process.waitFor(action.timeoutMillis(), TimeUnit.MILLISECONDS)) {
                timedOut = true;
                process.destroyForcibly();
                if (!process.waitFor(STREAM_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                    exitCode = -1;
                } else {
                    exitCode = process.exitValue();
                }
            } else {
                exitCode = process.exitValue();
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            process.destroyForcibly();
            interrupted = true;
            exitCode = -1;
        }
        BoundedOutput out;
        BoundedOutput err;
        try {
            out = output(stdout);
            err = output(stderr);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            interrupted = true;
            out = emptyOutput("stream read interrupted");
            err = emptyOutput("stream read interrupted");
        }
        readers.shutdownNow();
        return new CommandResult(
                action,
                command,
                exitCode,
                timedOut,
                interrupted,
                out,
                err
        );
    }

    private static Process startProcess(String command) throws IOException {
        return new ProcessBuilder("su", "-c", command)
                .redirectErrorStream(false)
                .start();
    }

    private static BoundedOutput output(Future<BoundedOutput> future)
            throws InterruptedException {
        try {
            return future.get(STREAM_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS);
        } catch (ExecutionException | TimeoutException error) {
            return emptyOutput("stream read failed: " + error.getClass().getSimpleName());
        }
    }

    private static BoundedOutput emptyOutput(String message) {
        return new BoundedOutput(message, false);
    }

    @FunctionalInterface
    interface ProcessStarter {
        Process start(String command) throws IOException;
    }
}
