package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import org.junit.Test;

public final class BridgeProtocolTest {
    @Test
    public void pairedV2RequiresTheEmbeddedBuildIdentity() throws BridgeFailure {
        BridgeProtocol protocol = BridgeProtocol.pairedV2();

        protocol.requireExpectedBuild(BuildConfig.PREVIEW_BUILD_ID);
        BridgeFailure mismatch = assertThrows(
                BridgeFailure.class,
                () -> protocol.requireExpectedBuild("different-build"));

        assertEquals("handshake", mismatch.requestId());
        assertEquals(ErrorCode.BUILD_MISMATCH, mismatch.code());
    }

    @Test
    public void pairedV2RejectsNonCanonicalBuildIdentity() throws BridgeFailure {
        BridgeProtocol protocol = BridgeProtocol.pairedV2();

        BridgeFailure rejected = assertThrows(
                BridgeFailure.class,
                () -> protocol.requireExpectedBuild("bad build/id"));

        assertEquals(ErrorCode.INVALID_REQUEST, rejected.code());
    }

    @Test
    public void legacyV1AllowsReadsButRejectsMutationBeforeBridgeUse()
            throws BridgeFailure {
        BridgeProtocol legacy = BridgeProtocol.legacyV1();
        ParsedCommand read = CommandParser.parse(
                new String[] {"probe-package", "com.example.read"});
        ParsedCommand mutation = CommandParser.parse(
                new String[] {"set-enabled", "com.example.read", "disabled_user"});

        legacy.requireAllowed(read);
        BridgeFailure rejected = assertThrows(
                BridgeFailure.class,
                () -> Main.execute(mutation, null, legacy));

        assertEquals("set-enabled", rejected.requestId());
        assertEquals(ErrorCode.BUILD_MISMATCH, rejected.code());
    }
}
