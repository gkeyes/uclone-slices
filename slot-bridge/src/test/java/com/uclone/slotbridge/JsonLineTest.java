package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import java.io.IOException;
import java.io.InputStream;
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
    public void launchAcknowledgementUsesItsFixedRequestId() throws BridgeFailure {
        assertEquals(
                "{\"schemaVersion\":1,\"requestId\":\"launch-package\",\"ok\":true,"
                        + "\"payload\":{\"type\":\"ack\"}}\n",
                JsonLine.ack("launch-package"));
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

    @Test
    public void emitsExactPairedV2CrossLanguageGolden()
            throws BridgeFailure, IOException {
        BridgeProtocol protocol = BridgeProtocol.pairedV2();
        PackageSnapshot snapshot = new PackageSnapshot(
                "com.example.fixture",
                10123,
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                7,
                "fixture",
                "/data/app/com.example.fixture/base.apk",
                PackagePolicy.pathsFor("com.example.fixture"),
                new PackageManagerInodes(111, 222),
                EnabledState.DEFAULT,
                false,
                false,
                false,
                false,
                false);

        String encoded = JsonLine.packageSnapshot(snapshot, protocol);
        String expected = readResource("/package-success.json")
                .replace("__BUILD_ID__", BuildConfig.PREVIEW_BUILD_ID);

        assertEquals(expected, encoded);
    }

    @Test
    public void v2HandshakeAndErrorCarryEmbeddedBuildIdentity()
            throws BridgeFailure {
        BridgeProtocol protocol = BridgeProtocol.pairedV2();

        assertEquals(
                "{\"schemaVersion\":2,\"buildId\":\""
                        + BuildConfig.PREVIEW_BUILD_ID
                        + "\",\"requestId\":\"handshake\",\"ok\":true,"
                        + "\"payload\":{\"type\":\"ack\"}}\n",
                JsonLine.handshake(protocol));
        assertEquals(
                "{\"schemaVersion\":2,\"buildId\":\""
                        + BuildConfig.PREVIEW_BUILD_ID
                        + "\",\"requestId\":\"handshake\",\"ok\":false,"
                        + "\"errorCode\":\"build_mismatch\"}\n",
                JsonLine.error("handshake", ErrorCode.BUILD_MISMATCH, protocol));
    }

    private static String readResource(String name) throws IOException {
        try (InputStream stream = JsonLineTest.class.getResourceAsStream(name)) {
            if (stream == null) {
                throw new IOException("missing test resource " + name);
            }
            return new String(stream.readAllBytes(), StandardCharsets.UTF_8);
        }
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
