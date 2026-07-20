package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import java.nio.charset.StandardCharsets;
import org.junit.Test;

public final class JsonLineTest {
    @Test
    public void emitsOneStrictDeviceJsonLineWhenProbeSucceeds() throws BridgeFailure {
        DeviceSnapshot snapshot = new DeviceSnapshot(false, 36);

        String encoded = JsonLine.device(snapshot);

        assertEquals(
                "{\"schemaVersion\":1,\"requestId\":\"device\",\"ok\":true,"
                        + "\"payload\":{\"type\":\"device\",\"userId\":0,"
                        + "\"unlocked\":false,\"apiLevel\":36}}\n",
                encoded);
    }

    @Test
    public void emitsMinimalTypedGateJsonLine() throws BridgeFailure {
        GateSnapshot snapshot = new GateSnapshot(EnabledState.DISABLED_USER, true);

        String encoded = JsonLine.gate(snapshot);

        assertEquals(
                "{\"schemaVersion\":1,\"requestId\":\"gate\",\"ok\":true,"
                        + "\"payload\":{\"type\":\"gate\","
                        + "\"enabledState\":\"disabled_user\",\"suspended\":true}}\n",
                encoded);
    }

    @Test
    public void escapesDynamicTextWithoutCreatingSecondJsonLine() throws BridgeFailure {
        PackageSnapshot snapshot = fixture("v\"1\nnext");

        String encoded = JsonLine.packageSnapshot(snapshot);

        assertTrue(encoded.contains("\"versionName\":\"v\\\"1\\nnext\""));
        assertEquals(encoded.length() - 1, encoded.indexOf('\n'));
    }

    @Test
    public void emitsCodeOnlyErrorWithoutDiagnosticText() throws BridgeFailure {
        String encoded = JsonLine.error("package", ErrorCode.PACKAGE_NOT_FOUND);

        assertEquals(
                "{\"schemaVersion\":1,\"requestId\":\"package\",\"ok\":false,"
                        + "\"errorCode\":\"package_not_found\"}\n",
                encoded);
    }

    @Test
    public void rejectsFrameWhenUtf8OutputExceedsBudget() throws BridgeFailure {
        PackageSnapshot snapshot = fixture("x".repeat(JsonLine.MAX_OUTPUT_BYTES));

        try {
            JsonLine.packageSnapshot(snapshot);
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.RESPONSE_TOO_LARGE, failure.code());
        }
    }

    @Test
    public void keepsEveryErrorFrameInsideUtf8Budget() throws BridgeFailure {
        byte[] encoded = JsonLine.error("request", ErrorCode.INTERNAL)
                .getBytes(StandardCharsets.UTF_8);

        assertTrue(encoded.length <= JsonLine.MAX_OUTPUT_BYTES);
    }

    private static PackageSnapshot fixture(String versionName) throws BridgeFailure {
        return new PackageSnapshot(
                TargetProfile.PACKAGE,
                10123,
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                1,
                versionName,
                "/data/app/example/base.apk",
                PackagePolicy.pathsFor(TargetProfile.PACKAGE),
                new PackageManagerInodes(111, 222),
                EnabledState.DEFAULT,
                false,
                false,
                false,
                false,
                false);
    }
}
