package com.uclone.slotbridge;

final class BridgeProtocol {
    static final int LEGACY_SCHEMA_VERSION = 1;
    static final int PAIRED_SCHEMA_VERSION = 2;

    private final int schemaVersion;
    private final String buildId;

    private BridgeProtocol(int schemaVersion, String buildId) {
        this.schemaVersion = schemaVersion;
        this.buildId = buildId;
    }

    static BridgeProtocol legacyV1() {
        return new BridgeProtocol(LEGACY_SCHEMA_VERSION, null);
    }

    static BridgeProtocol pairedV2() throws BridgeFailure {
        validateBuildId(BuildConfig.PREVIEW_BUILD_ID);
        return new BridgeProtocol(PAIRED_SCHEMA_VERSION, BuildConfig.PREVIEW_BUILD_ID);
    }

    int schemaVersion() {
        return schemaVersion;
    }

    String buildId() {
        return buildId;
    }

    boolean isPaired() {
        return schemaVersion == PAIRED_SCHEMA_VERSION;
    }

    void requireExpectedBuild(String expectedBuildId) throws BridgeFailure {
        validateBuildId(expectedBuildId);
        if (!BuildConfig.PREVIEW_BUILD_ID.equals(expectedBuildId)) {
            throw new BridgeFailure("handshake", ErrorCode.BUILD_MISMATCH);
        }
    }

    void requireAllowed(ParsedCommand command) throws BridgeFailure {
        if (!isPaired() && !command.readOnly()) {
            throw new BridgeFailure(command.requestId(), ErrorCode.BUILD_MISMATCH);
        }
    }

    private static void validateBuildId(String buildId) throws BridgeFailure {
        if (buildId == null || buildId.isEmpty() || buildId.length() > 160) {
            throw new BridgeFailure("handshake", ErrorCode.INVALID_REQUEST);
        }
        for (int index = 0; index < buildId.length(); index++) {
            char character = buildId.charAt(index);
            boolean valid = (character >= 'A' && character <= 'Z')
                    || (character >= 'a' && character <= 'z')
                    || (character >= '0' && character <= '9')
                    || character == '.'
                    || character == '_'
                    || character == '-';
            if (!valid) {
                throw new BridgeFailure("handshake", ErrorCode.INVALID_REQUEST);
            }
        }
    }
}
