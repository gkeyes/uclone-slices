package com.uclone.slotbridge;

final class ParsedCommand {
    private final CommandKind kind;
    private final EnabledState enabledState;
    private final boolean suspended;

    private ParsedCommand(CommandKind kind, EnabledState enabledState, boolean suspended) {
        this.kind = kind;
        this.enabledState = enabledState;
        this.suspended = suspended;
    }

    static ParsedCommand probeDevice() {
        return new ParsedCommand(CommandKind.PROBE_DEVICE, null, false);
    }

    static ParsedCommand probePackage() {
        return new ParsedCommand(CommandKind.PROBE_PACKAGE, null, false);
    }

    static ParsedCommand probeGate() {
        return new ParsedCommand(CommandKind.PROBE_GATE, null, false);
    }

    static ParsedCommand setEnabled(EnabledState state) {
        return new ParsedCommand(CommandKind.SET_ENABLED, state, false);
    }

    static ParsedCommand setSuspended(boolean value) {
        return new ParsedCommand(CommandKind.SET_SUSPENDED, null, value);
    }

    CommandKind kind() {
        return kind;
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
