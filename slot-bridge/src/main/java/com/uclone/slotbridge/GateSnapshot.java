package com.uclone.slotbridge;

final class GateSnapshot {
    private final EnabledState enabledState;
    private final boolean suspended;

    GateSnapshot(EnabledState enabledState, boolean suspended) {
        this.enabledState = enabledState;
        this.suspended = suspended;
    }

    EnabledState enabledState() {
        return enabledState;
    }

    boolean suspended() {
        return suspended;
    }
}
