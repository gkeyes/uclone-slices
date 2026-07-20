package com.uclone.slotbridge;

final class BridgeFailure extends Exception {
    private static final long serialVersionUID = 1L;

    private final String requestId;
    private final ErrorCode code;

    BridgeFailure(String requestId, ErrorCode code) {
        this.requestId = requestId;
        this.code = code;
    }

    String requestId() {
        return requestId;
    }

    ErrorCode code() {
        return code;
    }
}
