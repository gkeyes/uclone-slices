package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import org.junit.Test;

public final class CommandParserTest {
    private static final String PACKAGE = "com.example.one";
    private static final String SIGNATURE =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    private static final String CODE_PATH = "/data/app/example/base.apk";

    @Test
    public void parsesEveryFixedCommandWhenArgumentsAreExact() throws BridgeFailure {
        String packageName = PACKAGE;

        ParsedCommand device = CommandParser.parse(new String[] {"probe-device"});
        ParsedCommand probe = CommandParser.parse(new String[] {"probe-package", packageName});
        ParsedCommand gate = CommandParser.parse(new String[] {"probe-gate", packageName});
        ParsedCommand launch = CommandParser.parse(contract("launch-package", packageName));
        ParsedCommand enabled = CommandParser.parse(
                new String[] {"set-enabled", packageName, "disabled_user"});
        ParsedCommand restoredEnabled = CommandParser.parse(
                contractWithValue("restore-enabled", packageName, "default"));
        ParsedCommand suspended = CommandParser.parse(
                contractWithValue("restore-suspended", packageName, "false"));

        assertEquals(CommandKind.PROBE_DEVICE, device.kind());
        assertEquals(CommandKind.PROBE_PACKAGE, probe.kind());
        assertEquals(CommandKind.PROBE_GATE, gate.kind());
        assertEquals(CommandKind.LAUNCH_PACKAGE, launch.kind());
        assertEquals("launch-package", launch.requestId());
        assertEquals(EnabledState.DISABLED_USER, enabled.enabledState());
        assertEquals(EnabledState.DEFAULT, restoredEnabled.enabledState());
        assertFalse(suspended.suspended());
    }

    @Test
    public void acceptsDifferentValidatedPackagesWithoutRebuild() throws BridgeFailure {
        ParsedCommand one = CommandParser.parse(new String[] {"probe-package", PACKAGE});
        ParsedCommand two = CommandParser.parse(
                new String[] {"probe-package", "org.example.second"});

        assertEquals(PACKAGE, one.packageName());
        assertEquals("org.example.second", two.packageName());
    }

    @Test
    public void gateProbeRejectsInvalidPackageAndUserArgument() {
        assertFailure(
                new String[] {"probe-gate", "com.example.other/../bad"},
                ErrorCode.PACKAGE_NOT_ALLOWED);
        assertFailure(
                new String[] {"probe-gate", PACKAGE, "0"},
                ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void launchRequiresPackageAndCanonicalExpectedIdentity() throws BridgeFailure {
        ParsedCommand launch = CommandParser.parse(
                contract("launch-package", "org.example.launchable"));

        assertEquals("org.example.launchable", launch.packageName());
        assertTrue(launch.expectedIdentity() != null);
        assertFailure(
                new String[] {"launch-package", "org.example.launchable"},
                ErrorCode.INVALID_REQUEST);
        assertFailure(
                contract("launch-package", "/data/user/0/org.example.launchable"),
                ErrorCode.PACKAGE_NOT_ALLOWED);
        String[] nonCanonicalUid = contract("launch-package", "org.example.launchable");
        nonCanonicalUid[2] = "010123";
        assertFailure(
                nonCanonicalUid,
                ErrorCode.INVALID_REQUEST);
        String[] uppercaseSignature = contract("launch-package", "org.example.launchable");
        uppercaseSignature[3] = SIGNATURE.toUpperCase();
        assertFailure(
                uppercaseSignature,
                ErrorCode.INVALID_REQUEST);
        String[] zeroVersion = contract("launch-package", "org.example.launchable");
        zeroVersion[4] = "0";
        assertFailure(
                zeroVersion,
                ErrorCode.INVALID_REQUEST);
        String[] tabbedCodePath = contract("launch-package", "org.example.launchable");
        tabbedCodePath[5] = "/data/app/example\tother/base.apk";
        assertFailure(tabbedCodePath, ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void rejectsPathShapedPackageWhenCallerSuppliesPath() {
        String[] argv = {"probe-package", "/data/user/0/" + PACKAGE};

        assertFailure(argv, ErrorCode.PACKAGE_NOT_ALLOWED);
    }

    @Test
    public void rejectsExtraArgumentsWhenCommandShapeIsOtherwiseValid() {
        String[] argv = {"probe-device", "--user", "0"};

        assertFailure(argv, ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void parsesRestoreBooleanOnlyWhenLowercaseAndCanonical() throws BridgeFailure {
        String packageName = PACKAGE;

        ParsedCommand trueValue = CommandParser.parse(
                contractWithValue("restore-suspended", packageName, "true"));

        assertTrue(trueValue.suspended());
        assertFailure(
                contractWithValue("restore-suspended", packageName, "TRUE"),
                ErrorCode.INVALID_REQUEST);
    }

    @Test
    public void uncheckedEnabledMutationCanOnlyAcquireDisabledUser() {
        assertFailure(
                new String[] {"set-enabled", PACKAGE, "enabled"},
                ErrorCode.INVALID_REQUEST);
        assertFailure(
                new String[] {"set-enabled", PACKAGE, "default"},
                ErrorCode.INVALID_REQUEST);
    }

    private static String[] contract(String operation, String packageName) {
        return new String[] {
            operation, packageName, "10123", SIGNATURE, "7", CODE_PATH, "111", "222"
        };
    }

    private static String[] contractWithValue(
            String operation, String packageName, String value) {
        String[] contract = contract(operation, packageName);
        String[] result = new String[contract.length + 1];
        System.arraycopy(contract, 0, result, 0, contract.length);
        result[result.length - 1] = value;
        return result;
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
