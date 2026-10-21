package com.uclone.slices.v2.gate;

import android.content.Context;
import android.net.LocalSocket;
import android.net.LocalSocketAddress;
import android.system.Os;
import android.system.StructStat;

import com.uclone.slices.v2.BuildConfig;

import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.File;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.ArrayBlockingQueue;

/** Root communication runs on dedicated threads and never under an Android service lock. */
final class SystemBridge {
    private static final String ADDRESS = "uclone.slices.launch.v1";
    private static final int FRAME_LIMIT = 64 * 1024;
    // The queue matches Runtime's concurrent connection bound; overflow disconnects and
    // cancels authority instead of dropping a cancellation or blocking system_server.
    private static final int QUEUE_LIMIT = 32;
    final LaunchPolicyCache cache = new LaunchPolicyCache(BuildConfig.VERSION_NAME, this::send);
    private volatile Connection current;
    private volatile boolean disabled;

    SystemBridge() {
        try {
            File file = new File("/data/system/uclone-slices-v2/enrollment.json");
            StructStat metadata = Os.lstat(file.getPath());
            if (metadata.st_uid == 0 && (metadata.st_mode & 0022) == 0 && metadata.st_size <= FRAME_LIMIT) {
                cache.discovery(new JSONObject(new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8)));
            }
        } catch (Exception ignored) {
            // No launch permission comes from discovery. Zygisk separately discovers
            // configured packages and fails their specialization closed without Root.
        }
    }

    void start(Context context) {
        try { cache.managerUid(context.getPackageManager().getApplicationInfo(LaunchPolicyCache.MANAGER, 0).uid); }
        catch (Exception ignored) { fail(); return; }
        Thread thread = new Thread(this::connectLoop, "Slice launch control");
        thread.setDaemon(true);
        thread.start();
    }

    void fail() {
        cache.disconnected();
        Connection connection = current;
        if (connection != null) connection.close();
    }

    void disable() { disabled = true; fail(); }

    private void send(JSONObject event) {
        Connection connection = current;
        if (connection == null || !connection.outgoing.offer(event)) fail();
    }

    private void connectLoop() {
        // Reconnect backoff is transport recovery, never an App departure/return timer.
        long backoff = 250;
        while (!disabled) {
            Connection connection = null;
            try {
                LocalSocket socket = new LocalSocket();
                socket.connect(new LocalSocketAddress(ADDRESS, LocalSocketAddress.Namespace.ABSTRACT));
                if (socket.getPeerCredentials().getUid() != 0) {
                    socket.close();
                    throw new SecurityException("launch server is not Root");
                }
                connection = new Connection(socket);
                current = connection;
                JSONObject subscribe = new JSONObject();
                subscribe.put("op", "subscribe");
                subscribe.put("build_id", BuildConfig.VERSION_NAME);
                connection.write(subscribe);
                Connection writerConnection = connection;
                Thread writer = new Thread(() -> writerConnection.writeLoop(), "Slice launch events");
                writer.setDaemon(true);
                connection.writer = writer;
                writer.start();
                BufferedReader reader = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.UTF_8));
                for (;;) {
                    String frame = boundedLine(reader);
                    long revision = cache.update(new JSONObject(frame));
                    if (!connection.outgoing.offer(LaunchPolicyCache.message("ack", "revision", revision))) {
                        throw new IllegalStateException("launch event queue full");
                    }
                    backoff = 250;
                }
            } catch (Exception ignored) {
                cache.disconnected();
            } finally {
                if (connection != null) connection.close();
                if (current == connection) current = null;
            }
            try { Thread.sleep(backoff); }
            catch (InterruptedException stopped) { Thread.currentThread().interrupt(); return; }
            backoff = Math.min(backoff * 2, 4000);
        }
    }

    private static String boundedLine(BufferedReader reader) throws Exception {
        StringBuilder text = new StringBuilder();
        for (int count = 0; count < FRAME_LIMIT; count++) {
            int value = reader.read();
            if (value < 0) throw new IllegalStateException("launch connection ended");
            if (value == '\n') return text.toString();
            text.append((char) value);
        }
        throw new IllegalStateException("launch frame too large");
    }

    private static final class Connection {
        final LocalSocket socket;
        final ArrayBlockingQueue<JSONObject> outgoing = new ArrayBlockingQueue<>(QUEUE_LIMIT);
        final OutputStream output;
        volatile Thread writer;

        Connection(LocalSocket socket) throws Exception {
            this.socket = socket;
            output = socket.getOutputStream();
        }

        synchronized void write(JSONObject message) throws Exception {
            byte[] frame = (message.toString() + "\n").getBytes(StandardCharsets.UTF_8);
            if (frame.length > FRAME_LIMIT) throw new IllegalArgumentException("launch event too large");
            output.write(frame);
            output.flush();
        }

        void writeLoop() {
            try { for (;;) write(outgoing.take()); }
            catch (Exception ignored) { close(); }
        }

        void close() {
            try { socket.close(); } catch (Exception ignored) { }
            Thread thread = writer;
            if (thread != null && thread != Thread.currentThread()) thread.interrupt();
            outgoing.clear();
        }
    }
}
