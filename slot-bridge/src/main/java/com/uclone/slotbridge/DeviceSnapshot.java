package com.uclone.slotbridge;

final class DeviceSnapshot {
    private final boolean unlocked;
    private final int apiLevel;

    DeviceSnapshot(boolean unlocked, int apiLevel) {
        this.unlocked = unlocked;
        this.apiLevel = apiLevel;
    }

    boolean unlocked() {
        return unlocked;
    }

    int apiLevel() {
        return apiLevel;
    }
}
