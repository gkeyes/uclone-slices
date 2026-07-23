package com.uclone.slotbridge;

enum ErrorCode {
    INVALID_REQUEST("invalid_request"),
    PACKAGE_NOT_ALLOWED("package_not_allowed"),
    USER_NOT_ALLOWED("user_not_allowed"),
    INVALID_RESPONSE("invalid_response"),
    REQUEST_MISMATCH("request_mismatch"),
    RESPONSE_TOO_LARGE("response_too_large"),
    RUNNER_UNAVAILABLE("runner_unavailable"),
    COMMAND_FAILED("command_failed"),
    DEVICE_LOCKED("device_locked"),
    PACKAGE_NOT_FOUND("package_not_found"),
    LAUNCH_ENTRY_NOT_FOUND("launch_entry_not_found"),
    IDENTITY_CHANGED("identity_changed"),
    PACKAGE_STATE_CHANGED("package_state_changed"),
    PENDING_SESSION("pending_session"),
    BUILD_MISMATCH("build_mismatch"),
    INTERNAL("internal");

    private final String wireName;

    ErrorCode(String wireName) {
        this.wireName = wireName;
    }

    String wireName() {
        return wireName;
    }
}
