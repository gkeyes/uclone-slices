package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.fail;

import org.junit.Test;

public final class ExpectedPackageIdentityTest {
    private static final String SIGNATURE =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    @Test
    public void matchesOnlyTheCompleteStablePackageContract() throws BridgeFailure {
        ExpectedPackageIdentity expected =
                expected();

        expected.requireMatches(snapshot(10123, SIGNATURE, 7, CODE_PATH, 111, 222, false),
                "launch-package");
        assertMismatch(expected, snapshot(10124, SIGNATURE, 7, CODE_PATH, 111, 222, false),
                ErrorCode.IDENTITY_CHANGED);
        assertMismatch(expected, snapshot(10123, "ab".repeat(32), 7, CODE_PATH, 111, 222, false),
                ErrorCode.IDENTITY_CHANGED);
        assertMismatch(expected, snapshot(10123, SIGNATURE, 8, CODE_PATH, 111, 222, false),
                ErrorCode.PACKAGE_STATE_CHANGED);
        assertMismatch(expected, snapshot(10123, SIGNATURE, 7, OTHER_CODE_PATH, 111, 222, false),
                ErrorCode.PACKAGE_STATE_CHANGED);
        assertMismatch(expected, snapshot(10123, SIGNATURE, 7, CODE_PATH, 333, 222, false),
                ErrorCode.PACKAGE_STATE_CHANGED);
        assertMismatch(expected, snapshot(10123, SIGNATURE, 7, CODE_PATH, 111, 444, false),
                ErrorCode.PACKAGE_STATE_CHANGED);
        assertMismatch(expected, snapshot(10123, SIGNATURE, 7, CODE_PATH, 111, 222, true),
                ErrorCode.PENDING_SESSION);
    }

    private static final String CODE_PATH = "/data/app/example/base.apk";
    private static final String OTHER_CODE_PATH = "/data/app/reinstalled/base.apk";

    private static ExpectedPackageIdentity expected() throws BridgeFailure {
        return ExpectedPackageIdentity.parse(
                "launch-package", "10123", SIGNATURE, "7", CODE_PATH, "111", "222");
    }

    private static void assertMismatch(
            ExpectedPackageIdentity expected, PackageSnapshot actual, ErrorCode code) {
        try {
            expected.requireMatches(actual, "launch-package");
            fail("expected contract mismatch");
        } catch (BridgeFailure failure) {
            assertEquals(code, failure.code());
        }
    }

    private static PackageSnapshot snapshot(
            int uid,
            String signature,
            long version,
            String codePath,
            long ceInode,
            long deInode,
            boolean pendingInstall) {
        return new PackageSnapshot(
                "com.example.launchable",
                uid,
                signature,
                version,
                "1.0",
                codePath,
                new PackagePaths(
                        "/data/user/0/com.example.launchable",
                        "/data/user_de/0/com.example.launchable"),
                new PackageManagerInodes(ceInode, deInode),
                EnabledState.DEFAULT,
                false,
                pendingInstall,
                false,
                false,
                false);
    }
}
