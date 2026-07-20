package com.uclone.slotbridge;

enum EnabledState {
    DEFAULT("default", 0),
    ENABLED("enabled", 1),
    DISABLED("disabled", 2),
    DISABLED_USER("disabled_user", 3),
    DISABLED_UNTIL_USED("disabled_until_used", 4);

    private final String wireName;
    private final int platformValue;

    EnabledState(String wireName, int platformValue) {
        this.wireName = wireName;
        this.platformValue = platformValue;
    }

    String wireName() {
        return wireName;
    }

    int platformValue() {
        return platformValue;
    }

    static EnabledState fromPlatformValue(int value, String requestId) throws BridgeFailure {
        for (EnabledState state : values()) {
            if (state.platformValue == value) {
                return state;
            }
        }
        throw new BridgeFailure(requestId, ErrorCode.INVALID_RESPONSE);
    }
}
