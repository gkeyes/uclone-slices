package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;

import java.nio.charset.StandardCharsets;
import java.util.List;
import org.junit.Test;

public final class SigningIdentityTest {
    @Test
    public void returnsCertificateDigestWhenThereIsOneSigner() throws BridgeFailure {
        byte[] certificate = "certificate-a".getBytes(StandardCharsets.US_ASCII);

        String identity = SigningIdentity.aggregate(List.of(certificate));

        assertEquals(
                "bb313ab8861777efaca8039ba2048930576697261f5d3e3ca4a738bb8a3b8c8e",
                identity);
    }

    @Test
    public void aggregatesMultipleSignersIndependentlyOfInputOrder() throws BridgeFailure {
        byte[] first = "certificate-a".getBytes(StandardCharsets.US_ASCII);
        byte[] second = "certificate-b".getBytes(StandardCharsets.US_ASCII);

        String forward = SigningIdentity.aggregate(List.of(first, second));
        String reverse = SigningIdentity.aggregate(List.of(second, first));

        assertEquals(forward, reverse);
        assertEquals(
                "fa05a701d301932a3777d51280299b18fc4935f97a48dc4d1594651237e0fdaa",
                forward);
    }

    @Test
    public void rejectsDuplicateSignerCertificates() throws BridgeFailure {
        byte[] certificate = "certificate-a".getBytes(StandardCharsets.US_ASCII);

        try {
            SigningIdentity.aggregate(List.of(certificate, certificate));
            org.junit.Assert.fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }
}
