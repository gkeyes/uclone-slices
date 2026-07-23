package com.uclone.slotbridge;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;

final class BridgeServer {
    private static final int MAX_REQUEST_CHARS = 1024;

    private BridgeServer() {}

    static boolean isServeRequest(String[] argv) {
        return argv != null && argv.length == 1 && "serve".equals(argv[0]);
    }

    static boolean isPairedServeRequest(String[] argv) {
        return argv != null && argv.length == 2 && "serve-v2".equals(argv[0]);
    }

    static void serve(AndroidBridge bridge) throws IOException {
        serve(bridge, BridgeProtocol.legacyV1());
    }

    static void serve(AndroidBridge bridge, BridgeProtocol protocol) throws IOException {
        BufferedReader reader = new BufferedReader(
                new InputStreamReader(System.in, StandardCharsets.UTF_8));
        BufferedWriter writer = new BufferedWriter(
                new OutputStreamWriter(System.out, StandardCharsets.UTF_8));
        if (protocol.isPaired()) {
            try {
                writer.write(JsonLine.handshake(protocol));
                writer.flush();
            } catch (BridgeFailure failure) {
                throw new IOException("could not encode bridge handshake", failure);
            }
        }
        String line;
        while ((line = reader.readLine()) != null) {
            writer.write(executeLine(line, bridge, protocol));
            writer.flush();
        }
    }

    static String[] parseLine(String line) throws BridgeFailure {
        if (line == null || line.isEmpty() || line.length() > MAX_REQUEST_CHARS) {
            throw new BridgeFailure("request", ErrorCode.INVALID_REQUEST);
        }
        String[] argv = line.split("\\t", -1);
        for (String argument : argv) {
            if (argument.isEmpty()) {
                throw new BridgeFailure("request", ErrorCode.INVALID_REQUEST);
            }
        }
        return argv;
    }

    private static String executeLine(String line, AndroidBridge bridge) {
        return executeLine(line, bridge, BridgeProtocol.legacyV1());
    }

    private static String executeLine(
            String line, AndroidBridge bridge, BridgeProtocol protocol) {
        try {
            return Main.execute(CommandParser.parse(parseLine(line)), bridge, protocol);
        } catch (BridgeFailure failure) {
            return Main.error(failure.requestId(), failure.code(), protocol);
        } catch (Throwable failure) {
            return Main.error("request", ErrorCode.INTERNAL, protocol);
        }
    }
}
