package com.uclone.slotprobe;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

final class ProbeFileIo {
    private ProbeFileIo() {
    }

    static void writeSynced(File file, String value) {
        File parent = file.getParentFile();
        if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) {
            throw new IllegalStateException("Failed to create directory for " + file);
        }
        try (FileOutputStream output = new FileOutputStream(file, false)) {
            output.write(value.getBytes(StandardCharsets.UTF_8));
            output.getFD().sync();
        } catch (IOException error) {
            throw new IllegalStateException("Failed to write " + file, error);
        }
    }

    static void replaceSynced(File file, String value) {
        File parent = file.getParentFile();
        if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) {
            throw new IllegalStateException("Failed to create directory for " + file);
        }
        File temporary = new File(parent, file.getName() + ".tmp");
        writeSynced(temporary, value);
        if (!temporary.renameTo(file)) {
            temporary.delete();
            throw new IllegalStateException("Failed to atomically replace " + file);
        }
    }

    static String read(File file) {
        if (!file.isFile()) {
            return "<missing>";
        }
        try (FileInputStream input = new FileInputStream(file);
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4_096];
            int count;
            while ((count = input.read(buffer)) != -1) {
                output.write(buffer, 0, count);
            }
            return output.toString(StandardCharsets.UTF_8.name());
        } catch (IOException error) {
            throw new IllegalStateException("Failed to read " + file, error);
        }
    }

    static String canonicalPath(File file) {
        try {
            return file.getCanonicalPath();
        } catch (IOException error) {
            throw new IllegalStateException("Failed to resolve " + file, error);
        }
    }
}
