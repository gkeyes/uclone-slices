package com.uclone.slotbridge;

final class ParsedCommand {
    private final CommandKind kind;
    private final String packageName;
    private final EnabledState enabledState;
    private final boolean suspended;
    private final ExpectedPackageIdentity expectedIdentity;

    private ParsedCommand(
            CommandKind kind,
            String packageName,
            EnabledState enabledState,
            boolean suspended,
            ExpectedPackageIdentity expectedIdentity) {
        this.kind = kind;
        this.packageName = packageName;
        this.enabledState = enabledState;
        this.suspended = suspended;
        this.expectedIdentity = expectedIdentity;
    }

    static ParsedCommand probeDevice() {
        return new ParsedCommand(CommandKind.PROBE_DEVICE, null, null, false, null);
    }

    static ParsedCommand probePackage(String packageName) {
        return new ParsedCommand(CommandKind.PROBE_PACKAGE, packageName, null, false, null);
    }

    static ParsedCommand probeGate(String packageName) {
        return new ParsedCommand(CommandKind.PROBE_GATE, packageName, null, false, null);
    }

    static ParsedCommand launchPackage(
            String packageName, ExpectedPackageIdentity expectedIdentity) {
        return new ParsedCommand(
                CommandKind.LAUNCH_PACKAGE, packageName, null, false, expectedIdentity);
    }

    static ParsedCommand setEnabled(String packageName, EnabledState state) {
        return new ParsedCommand(CommandKind.SET_ENABLED, packageName, state, false, null);
    }

    static ParsedCommand restoreEnabled(
            String packageName,
            EnabledState state,
            ExpectedPackageIdentity expectedIdentity) {
        return new ParsedCommand(
                CommandKind.RESTORE_ENABLED, packageName, state, false, expectedIdentity);
    }

    static ParsedCommand restoreSuspended(
            String packageName, boolean value, ExpectedPackageIdentity expectedIdentity) {
        return new ParsedCommand(
                CommandKind.RESTORE_SUSPENDED,
                packageName,
                null,
                value,
                expectedIdentity);
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

    ExpectedPackageIdentity expectedIdentity() {
        return expectedIdentity;
    }

    String requestId() {
        switch (kind) {
            case PROBE_DEVICE:
                return "device";
            case PROBE_PACKAGE:
                return "package";
            case PROBE_GATE:
                return "gate";
            case LAUNCH_PACKAGE:
                return "launch-package";
            case SET_ENABLED:
                return "set-enabled";
            case RESTORE_ENABLED:
                return "restore-enabled";
            case RESTORE_SUSPENDED:
                return "restore-suspended";
            default:
                throw new AssertionError();
        }
    }
}
