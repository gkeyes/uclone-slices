package com.uclone.slotbridge;

final class ExpectedPackageIdentity {
    private final int uid;
    private final String signatureSha256;
    private final long versionCode;
    private final String codePath;
    private final long baseCeInode;
    private final long baseDeInode;

    private ExpectedPackageIdentity(
            int uid,
            String signatureSha256,
            long versionCode,
            String codePath,
            long baseCeInode,
            long baseDeInode) {
        this.uid = uid;
        this.signatureSha256 = signatureSha256;
        this.versionCode = versionCode;
        this.codePath = codePath;
        this.baseCeInode = baseCeInode;
        this.baseDeInode = baseDeInode;
    }

    static ExpectedPackageIdentity parse(
            String requestId,
            String uid,
            String signature,
            String version,
            String codePath,
            String baseCeInode,
            String baseDeInode)
            throws BridgeFailure {
        int parsedUid = parseUid(requestId, uid);
        long parsedVersion = parsePositiveLong(requestId, version);
        if (!isLowerHexSha256(signature)) {
            throw invalid(requestId);
        }
        if (!PackagePolicy.isSafeCodePath(codePath)) {
            throw invalid(requestId);
        }
        return new ExpectedPackageIdentity(
                parsedUid,
                signature,
                parsedVersion,
                codePath,
                parsePositiveLong(requestId, baseCeInode),
                parsePositiveLong(requestId, baseDeInode));
    }

    void requireMatches(PackageSnapshot actual, String requestId) throws BridgeFailure {
        if (actual.pendingInstall()) {
            throw new BridgeFailure(requestId, ErrorCode.PENDING_SESSION);
        }
        if (uid != actual.uid() || !signatureSha256.equals(actual.signatureSha256())) {
            throw new BridgeFailure(requestId, ErrorCode.IDENTITY_CHANGED);
        }
        PackageManagerInodes inodes = actual.packageManagerInodes();
        if (versionCode != actual.versionCode()
                || !codePath.equals(actual.codePath())
                || baseCeInode != inodes.ceInode()
                || baseDeInode != inodes.deInode()) {
            throw new BridgeFailure(requestId, ErrorCode.PACKAGE_STATE_CHANGED);
        }
    }

    private static int parseUid(String requestId, String value) throws BridgeFailure {
        long parsed = parsePositiveLong(requestId, value);
        if (parsed < 10_000 || parsed > Integer.MAX_VALUE) {
            throw invalid(requestId);
        }
        return (int) parsed;
    }

    private static long parsePositiveLong(String requestId, String value) throws BridgeFailure {
        if (!isCanonicalDecimal(value)) {
            throw invalid(requestId);
        }
        try {
            long parsed = Long.parseLong(value);
            if (parsed <= 0) {
                throw invalid(requestId);
            }
            return parsed;
        } catch (NumberFormatException failure) {
            throw invalid(requestId);
        }
    }

    private static boolean isCanonicalDecimal(String value) {
        if (value.isEmpty() || (value.length() > 1 && value.charAt(0) == '0')) {
            return false;
        }
        for (int index = 0; index < value.length(); index++) {
            if (value.charAt(index) < '0' || value.charAt(index) > '9') {
                return false;
            }
        }
        return true;
    }

    private static boolean isLowerHexSha256(String value) {
        if (value.length() != 64) {
            return false;
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            if (!((character >= '0' && character <= '9')
                    || (character >= 'a' && character <= 'f'))) {
                return false;
            }
        }
        return true;
    }

    private static BridgeFailure invalid(String requestId) {
        return new BridgeFailure(requestId, ErrorCode.INVALID_REQUEST);
    }
}
