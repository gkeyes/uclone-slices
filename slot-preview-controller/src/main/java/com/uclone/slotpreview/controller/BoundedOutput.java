package com.uclone.slotpreview.controller;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

final class BoundedOutput {
    static final int MAX_BYTES = 32 * 1024;

    private final String text;
    private final boolean truncated;

    BoundedOutput(String text, boolean truncated) {
        this.text = text;
        this.truncated = truncated;
    }

    static BoundedOutput read(InputStream stream, int maxBytes) throws IOException {
        if (maxBytes < 1) {
            throw new IllegalArgumentException("maxBytes must be positive");
        }
        byte[] buffer = new byte[4096];
        byte[] kept = new byte[maxBytes];
        int keptBytes = 0;
        boolean truncated = false;
        int count;
        while ((count = stream.read(buffer)) != -1) {
            int copy = Math.min(count, maxBytes - keptBytes);
            if (copy > 0) {
                System.arraycopy(buffer, 0, kept, keptBytes, copy);
                keptBytes += copy;
            }
            if (copy < count) {
                truncated = true;
            }
        }
        return new BoundedOutput(
                new String(kept, 0, keptBytes, StandardCharsets.UTF_8),
                truncated
        );
    }

    String text() {
        return text;
    }

    boolean truncated() {
        return truncated;
    }
}
