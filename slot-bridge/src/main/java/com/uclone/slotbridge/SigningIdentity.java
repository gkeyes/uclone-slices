package com.uclone.slotbridge;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

final class SigningIdentity {
    private static final int MAX_SIGNERS = 16;
    private static final int MAX_CERTIFICATE_BYTES = 64 * 1024;
    private static final byte[] DOMAIN =
            "uclone-current-signers-v1\0".getBytes(StandardCharsets.US_ASCII);

    private SigningIdentity() {}

    static String aggregate(List<byte[]> certificates) throws BridgeFailure {
        if (certificates == null
                || certificates.isEmpty()
                || certificates.size() > MAX_SIGNERS) {
            throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
        }
        MessageDigest sha256 = sha256();
        List<byte[]> digests = new ArrayList<>(certificates.size());
        for (byte[] certificate : certificates) {
            if (certificate == null
                    || certificate.length == 0
                    || certificate.length > MAX_CERTIFICATE_BYTES) {
                throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
            }
            digests.add(sha256.digest(certificate));
        }
        digests.sort(SigningIdentity::compareUnsigned);
        for (int index = 1; index < digests.size(); index++) {
            if (Arrays.equals(digests.get(index - 1), digests.get(index))) {
                throw new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
            }
        }
        if (digests.size() == 1) {
            return toHex(digests.get(0));
        }
        sha256.reset();
        sha256.update(DOMAIN);
        sha256.update((byte) (digests.size() >>> 8));
        sha256.update((byte) digests.size());
        for (byte[] digest : digests) {
            sha256.update(digest);
        }
        return toHex(sha256.digest());
    }

    private static MessageDigest sha256() throws BridgeFailure {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException failure) {
            throw new BridgeFailure("package", ErrorCode.INTERNAL);
        }
    }

    private static int compareUnsigned(byte[] left, byte[] right) {
        int limit = Math.min(left.length, right.length);
        for (int index = 0; index < limit; index++) {
            int comparison = Integer.compare(left[index] & 0xff, right[index] & 0xff);
            if (comparison != 0) {
                return comparison;
            }
        }
        return Integer.compare(left.length, right.length);
    }

    private static String toHex(byte[] bytes) {
        char[] encoded = new char[bytes.length * 2];
        char[] alphabet = "0123456789abcdef".toCharArray();
        for (int index = 0; index < bytes.length; index++) {
            int value = bytes[index] & 0xff;
            encoded[index * 2] = alphabet[value >>> 4];
            encoded[index * 2 + 1] = alphabet[value & 0x0f];
        }
        return new String(encoded);
    }
}
