package com.uclone.slotbridge;

import java.util.Arrays;

final class CommandParser {
    private static final String[] PROBE_DEVICE = {"probe-device"};
    private static final String[] PROBE_PACKAGE = {
        "probe-package", PackagePolicy.ALLOWED_PACKAGE
    };
    private static final String[] PROBE_GATE = {
        "probe-gate", PackagePolicy.ALLOWED_PACKAGE
    };

    private CommandParser() {}

    static ParsedCommand parse(String[] argv) throws BridgeFailure {
        validateBounds(argv);
        if (Arrays.equals(argv, PROBE_DEVICE)) {
            return ParsedCommand.probeDevice();
        }
        if (Arrays.equals(argv, PROBE_PACKAGE)) {
            return ParsedCommand.probePackage();
        }
        if (Arrays.equals(argv, PROBE_GATE)) {
            return ParsedCommand.probeGate();
        }
        for (EnabledState state : EnabledState.values()) {
            String[] expected = {"set-enabled", PackagePolicy.ALLOWED_PACKAGE, state.wireName()};
            if (Arrays.equals(argv, expected)) {
                return ParsedCommand.setEnabled(state);
            }
        }
        if (Arrays.equals(
                argv,
                new String[] {"set-suspended", PackagePolicy.ALLOWED_PACKAGE, "true"})) {
            return ParsedCommand.setSuspended(true);
        }
        if (Arrays.equals(
                argv,
                new String[] {"set-suspended", PackagePolicy.ALLOWED_PACKAGE, "false"})) {
            return ParsedCommand.setSuspended(false);
        }
        rejectDisallowedPackage(argv);
        throw new BridgeFailure(requestIdFor(argv), ErrorCode.INVALID_REQUEST);
    }

    private static void validateBounds(String[] argv) throws BridgeFailure {
        if (argv == null || argv.length == 0 || argv.length > 3) {
            throw new BridgeFailure("request", ErrorCode.INVALID_REQUEST);
        }
        for (String argument : argv) {
            if (argument == null || argument.isEmpty() || argument.length() > 255) {
                throw new BridgeFailure("request", ErrorCode.INVALID_REQUEST);
            }
        }
    }

    private static void rejectDisallowedPackage(String[] argv) throws BridgeFailure {
        String requestId = requestIdFor(argv);
        if (("package".equals(requestId)
                        || "gate".equals(requestId)
                        || "set-enabled".equals(requestId)
                        || "set-suspended".equals(requestId))
                && argv.length >= 2
                && !PackagePolicy.isAllowed(argv[1])) {
            throw new BridgeFailure(requestId, ErrorCode.PACKAGE_NOT_ALLOWED);
        }
    }

    private static String requestIdFor(String[] argv) {
        if (argv.length == 0 || argv[0] == null) {
            return "request";
        }
        switch (argv[0]) {
            case "probe-device":
                return "device";
            case "probe-package":
                return "package";
            case "probe-gate":
                return "gate";
            case "set-enabled":
                return "set-enabled";
            case "set-suspended":
                return "set-suspended";
            default:
                return "request";
        }
    }
}
