package com.uclone.slotpreview.controller;

import static org.junit.Assert.assertTrue;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import org.junit.Test;

public final class RootCommandExecutorTest {
    @Test
    public void outputWaitInterruptionReturnsUnknownResult() throws Exception {
        CountDownLatch outputReadStarted = new CountDownLatch(1);
        Thread testThread = Thread.currentThread();
        Thread interrupter = new Thread(() -> {
            await(outputReadStarted);
            testThread.interrupt();
        });
        interrupter.start();

        CommandResult result;
        try {
            RootCommandExecutor executor = new RootCommandExecutor(
                    command -> new CompletedProcess(new BlockingInput(outputReadStarted))
            );
            result = executor.execute(PreviewCommand.Action.STATUS);
        } finally {
            Thread.interrupted();
            interrupter.join(2_000L);
        }

        assertTrue(result.interrupted());
        assertTrue(result.displayText().contains("结果：未知"));
        assertTrue(result.displayText().contains("Status/Reconcile"));
    }

    private static void await(CountDownLatch latch) {
        try {
            latch.await();
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
        }
    }

    private static final class BlockingInput extends InputStream {
        private final CountDownLatch readStarted;

        BlockingInput(CountDownLatch readStarted) {
            this.readStarted = readStarted;
        }

        @Override
        public int read() throws IOException {
            return block();
        }

        @Override
        public int read(byte[] buffer, int offset, int length) throws IOException {
            return block();
        }

        private int block() throws IOException {
            readStarted.countDown();
            try {
                new CountDownLatch(1).await();
                return -1;
            } catch (InterruptedException error) {
                throw new IOException("reader interrupted", error);
            }
        }
    }

    private static final class CompletedProcess extends Process {
        private final InputStream output;

        CompletedProcess(InputStream output) {
            this.output = output;
        }

        @Override
        public OutputStream getOutputStream() {
            return OutputStream.nullOutputStream();
        }

        @Override
        public InputStream getInputStream() {
            return output;
        }

        @Override
        public InputStream getErrorStream() {
            return new ByteArrayInputStream(new byte[0]);
        }

        @Override
        public int waitFor() {
            return 0;
        }

        @Override
        public boolean waitFor(long timeout, TimeUnit unit) {
            return true;
        }

        @Override
        public int exitValue() {
            return 0;
        }

        @Override
        public void destroy() {
        }
    }
}
