package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import org.junit.Test;

public final class CommandParserTest {
    @Test
    public void parsesEveryFixedCommandWhenArgumentsAreExact() throws BridgeFailure {
        String packageName = PackagePolicy.ALLOWED_PACKAGE;

        ParsedCommand device = CommandParser.parse(new String[] {"probe-device"});
        ParsedCommand probe = CommandParser.parse(new String[] {"probe-package", packageName});
        ParsedCommand gate = CommandParser.parse(new String[] {"probe-gate", packageName});
        ParsedCommand enabled = CommandParser.parse(
                new String[] {"set-enabled", packageName, "disabled_user"});
        ParsedCommand suspended = CommandParser.parse(
                new String[] {"set-suspended", packageName, "false"});

        assertEquals(CommandKind.PROBE_DEVICE, device.kind());
        assertEquals(CommandKind.PROBE_PACKAGE, probe.kind());
        assertEquals(CommandKind.PROBE_GATE, gate.kind());
        assertEquals(EnabledState.DISABLED_USER, enabled.enabledState());
        assertFalse(suspended.suspended());
    }

    @Test
    public void rejectsPackageWhenItIsOutsideAllowlist() {
        String[] argv = {"probe-package", "com.example.other"};

        assertFailure(argv, ErrorCode.PACKAGE_NOT_ALLOWED);
    }

    @Test
    public void gateProbeRejectsForeignPackageAndUserArgument() {
        assertFailure(
                new String[] {"probe-gate", "com.example.other"},
                ErrorCode.PACKAGE_NOT_ALLOWED);
        assertFailure(
                new String[] {"probe-gate", PackagePolicy.ALLOWED_PACKAGE, "0"},
                ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void rejectsPathShapedPackageWhenCallerSuppliesPath() {
        String[] argv = {"probe-package", "/data/user/0/" + PackagePolicy.ALLOWED_PACKAGE};

        assertFailure(argv, ErrorCode.PACKAGE_NOT_ALLOWED);
    }

    @Test
    public void rejectsExtraArgumentsWhenCommandShapeIsOtherwiseValid() {
        String[] argv = {"probe-device", "--user", "0"};

        assertFailure(argv, ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void parsesBooleanOnlyWhenLowercaseAndCanonical() throws BridgeFailure {
        String packageName = PackagePolicy.ALLOWED_PACKAGE;

        ParsedCommand trueValue = CommandParser.parse(
                new String[] {"set-suspended", packageName, "true"});

        assertTrue(trueValue.suspended());
        assertFailure(
                new String[] {"set-suspended", packageName, "TRUE"},
                ErrorCode.INVALID_REQUEST);
    }

    private static void assertFailure(String[] argv, ErrorCode expected) {
        try {
            CommandParser.parse(argv);
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(expected, failure.code());
        }
    }
}
