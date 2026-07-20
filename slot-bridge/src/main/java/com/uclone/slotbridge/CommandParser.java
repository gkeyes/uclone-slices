package com.uclone.slotbridge;

final class CommandParser {
    private CommandParser() {}

    static ParsedCommand parse(String[] argv) throws BridgeFailure {
        validateBounds(argv);
        if (argv.length == 1 && "probe-device".equals(argv[0])) {
            return ParsedCommand.probeDevice();
        }
        if (argv.length >= 2 && isPackageCommand(argv[0])) {
            requireAllowedPackage(argv[0], argv[1]);
        }
        if (argv.length == 2 && "probe-package".equals(argv[0])) {
            return ParsedCommand.probePackage(argv[1]);
        }
        if (argv.length == 2 && "probe-gate".equals(argv[0])) {
            return ParsedCommand.probeGate(argv[1]);
        }
        if (argv.length == 3 && "set-enabled".equals(argv[0])) {
            for (EnabledState state : EnabledState.values()) {
                if (state.wireName().equals(argv[2])) {
                    return ParsedCommand.setEnabled(argv[1], state);
                }
            }
        }
        if (argv.length == 3 && "set-suspended".equals(argv[0])) {
            if ("true".equals(argv[2]) || "false".equals(argv[2])) {
                return ParsedCommand.setSuspended(argv[1], Boolean.parseBoolean(argv[2]));
            }
        }
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

    private static boolean isPackageCommand(String command) {
        return "probe-package".equals(command)
                || "probe-gate".equals(command)
                || "set-enabled".equals(command)
                || "set-suspended".equals(command);
    }

    private static void requireAllowedPackage(String command, String packageName)
            throws BridgeFailure {
        if (!PackagePolicy.isAllowed(packageName)) {
            throw new BridgeFailure(requestIdFor(new String[] {command}),
                    ErrorCode.PACKAGE_NOT_ALLOWED);
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
