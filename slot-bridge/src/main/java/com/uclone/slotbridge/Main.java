package com.uclone.slotbridge;

import android.system.Os;

public final class Main {
    private static final String INTERNAL_ERROR =
            "{\"schemaVersion\":1,\"requestId\":\"request\",\"ok\":false,"
                    + "\"errorCode\":\"internal\"}\n";

    private Main() {}

    public static void main(String[] argv) {
        String output;
        BridgeProtocol protocol = BridgeProtocol.legacyV1();
        try {
            requireRoot();
            if (BridgeServer.isPairedServeRequest(argv)) {
                protocol = BridgeProtocol.pairedV2();
                protocol.requireExpectedBuild(argv[1]);
                BridgeServer.serve(AndroidBridge.connect(), protocol);
                return;
            }
            if (BridgeServer.isServeRequest(argv)) {
                BridgeServer.serve(AndroidBridge.connect(), protocol);
                return;
            }
            ParsedCommand command = CommandParser.parse(argv);
            protocol.requireAllowed(command);
            AndroidBridge bridge = AndroidBridge.connect();
            output = execute(command, bridge, protocol);
        } catch (BridgeFailure failure) {
            output = error(failure.requestId(), failure.code(), protocol);
        } catch (Throwable failure) {
            output = error("request", ErrorCode.INTERNAL, protocol);
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
        return execute(command, bridge, BridgeProtocol.legacyV1());
    }

    static String execute(
            ParsedCommand command, AndroidBridge bridge, BridgeProtocol protocol)
            throws BridgeFailure {
        protocol.requireAllowed(command);
        switch (command.kind()) {
            case PROBE_DEVICE:
                return JsonLine.device(bridge.probeDevice(), protocol);
            case PROBE_PACKAGE:
                return JsonLine.packageSnapshot(
                        bridge.probePackage(command.packageName()), protocol);
            case PROBE_GATE:
                return JsonLine.gate(bridge.probeGate(command.packageName()), protocol);
            case LAUNCH_PACKAGE:
                bridge.launchPackage(command.packageName(), command.expectedIdentity());
                return JsonLine.ack(command.requestId(), protocol);
            case SET_ENABLED:
                bridge.setEnabled(command.packageName(), command.enabledState());
                return JsonLine.ack(command.requestId(), protocol);
            case RESTORE_ENABLED:
                bridge.restoreEnabled(
                        command.packageName(),
                        command.enabledState(),
                        command.expectedIdentity());
                return JsonLine.ack(command.requestId(), protocol);
            case RESTORE_SUSPENDED:
                bridge.restoreSuspended(
                        command.packageName(),
                        command.suspended(),
                        command.expectedIdentity());
                return JsonLine.ack(command.requestId(), protocol);
            default:
                return INTERNAL_ERROR;
        }
    }

    static String error(String requestId, ErrorCode code) {
        return error(requestId, code, BridgeProtocol.legacyV1());
    }

    static String error(
            String requestId, ErrorCode code, BridgeProtocol protocol) {
        try {
            return JsonLine.error(requestId, code, protocol);
        } catch (BridgeFailure failure) {
            return INTERNAL_ERROR;
        }
    }
}
