package com.uclone.slotpreview.controller;

final class PreviewCommand {
    static final String TARGET_PACKAGE = TargetProfile.PACKAGE;
    static final String SLOTCTL_PATH = TargetProfile.SLOTCTL_PATH;

    enum Action {
        PROBE("探测 Runtime", false, 30_000L),
        ENROLL("登记目标基础槽", true, 120_000L),
        STATUS("读取目标状态", false, 30_000L),
        SWITCH_PREVIEW("切换到 Preview 槽", true, 1_800_000L),
        SWITCH_BASE("切回 Base 槽", true, 120_000L),
        RESCUE_BASE("救援并切回 Base", true, 600_000L);

        private final String label;
        private final boolean confirmationRequired;
        private final long timeoutMillis;

        Action(String label, boolean confirmationRequired, long timeoutMillis) {
            this.label = label;
            this.confirmationRequired = confirmationRequired;
            this.timeoutMillis = timeoutMillis;
        }

        String label() {
            return label;
        }

        boolean confirmationRequired() {
            return confirmationRequired;
        }

        long timeoutMillis() {
            return timeoutMillis;
        }
    }

    private PreviewCommand() {
    }

    static String build(Action action) {
        return switch (action) {
            case PROBE -> command("probe");
            case ENROLL -> command("enroll " + TARGET_PACKAGE);
            case STATUS -> command("status " + TARGET_PACKAGE);
            case SWITCH_PREVIEW -> command(
                    "switch " + TARGET_PACKAGE + " " + TargetProfile.PREVIEW_SLOT
            );
            case SWITCH_BASE -> command(
                    "switch " + TARGET_PACKAGE + " " + TargetProfile.BASE_SLOT
            );
            case RESCUE_BASE -> command("rescue " + TARGET_PACKAGE + " --to-base");
        };
    }

    private static String command(String arguments) {
        return SLOTCTL_PATH + " " + arguments;
    }

}
