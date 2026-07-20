package com.uclone.slotbridge;

final class ParsedCommand {
    private final CommandKind kind;
    private final String packageName;
    private final EnabledState enabledState;
    private final boolean suspended;

    private ParsedCommand(
            CommandKind kind,
            String packageName,
            EnabledState enabledState,
            boolean suspended) {
        this.kind = kind;
        this.packageName = packageName;
        this.enabledState = enabledState;
        this.suspended = suspended;
    }

    static ParsedCommand probeDevice() {
        return new ParsedCommand(CommandKind.PROBE_DEVICE, null, null, false);
    }

    static ParsedCommand probePackage(String packageName) {
        return new ParsedCommand(CommandKind.PROBE_PACKAGE, packageName, null, false);
    }

    static ParsedCommand probeGate(String packageName) {
        return new ParsedCommand(CommandKind.PROBE_GATE, packageName, null, false);
    }

    static ParsedCommand setEnabled(String packageName, EnabledState state) {
        return new ParsedCommand(CommandKind.SET_ENABLED, packageName, state, false);
    }

    static ParsedCommand setSuspended(String packageName, boolean value) {
        return new ParsedCommand(CommandKind.SET_SUSPENDED, packageName, null, value);
    }

    CommandKind kind() {
        return kind;
    }

    String packageName() {
        return packageName;
    }

    EnabledState enabledState() {
        return enabledState;
    }

    boolean suspended() {
        return suspended;
    }

    String requestId() {
        switch (kind) {
            case PROBE_DEVICE:
                return "device";
            case PROBE_PACKAGE:
                return "package";
            case PROBE_GATE:
                return "gate";
            case SET_ENABLED:
                return "set-enabled";
            case SET_SUSPENDED:
                return "set-suspended";
            default:
                throw new AssertionError();
        }
    }
}
