package com.uclone.slotbridge;

import android.system.Os;

public final class Main {
    private static final String INTERNAL_ERROR =
            "{\"schemaVersion\":1,\"requestId\":\"request\",\"ok\":false,"
                    + "\"errorCode\":\"internal\"}\n";

    private Main() {}

    public static void main(String[] argv) {
        String output;
        try {
            requireRoot();
            if (BridgeServer.isServeRequest(argv)) {
                BridgeServer.serve(AndroidBridge.connect());
                return;
            }
            ParsedCommand command = CommandParser.parse(argv);
            AndroidBridge bridge = AndroidBridge.connect();
            output = execute(command, bridge);
        } catch (BridgeFailure failure) {
            output = error(failure.requestId(), failure.code());
        } catch (Throwable failure) {
            output = INTERNAL_ERROR;
        }
        System.out.print(output);
    }

    private static void requireRoot() throws BridgeFailure {
        try {
            if (Os.geteuid() != 0) {
                throw new BridgeFailure("request", ErrorCode.RUNNER_UNAVAILABLE);
            }
        } catch (RuntimeException | LinkageError failure) {
            throw new BridgeFailure("request", ErrorCode.RUNNER_UNAVAILABLE);
        }
    }

    static String execute(ParsedCommand command, AndroidBridge bridge)
            throws BridgeFailure {
        switch (command.kind()) {
            case PROBE_DEVICE:
                return JsonLine.device(bridge.probeDevice());
            case PROBE_PACKAGE:
                return JsonLine.packageSnapshot(bridge.probePackage(command.packageName()));
            case PROBE_GATE:
                return JsonLine.gate(bridge.probeGate(command.packageName()));
            case SET_ENABLED:
                bridge.setEnabled(command.packageName(), command.enabledState());
                return JsonLine.ack(command.requestId());
            case SET_SUSPENDED:
                bridge.setSuspended(command.packageName(), command.suspended());
                return JsonLine.ack(command.requestId());
            default:
                return INTERNAL_ERROR;
        }
    }

    static String error(String requestId, ErrorCode code) {
        try {
            return JsonLine.error(requestId, code);
        } catch (BridgeFailure failure) {
            return INTERNAL_ERROR;
        }
    }
}
