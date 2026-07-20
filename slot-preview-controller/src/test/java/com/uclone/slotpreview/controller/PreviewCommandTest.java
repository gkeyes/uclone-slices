package com.uclone.slotpreview.controller;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;

import java.lang.reflect.Method;

import org.junit.Test;

public final class PreviewCommandTest {
    @Test
    public void buildsOnlySelectedTargetCommands() {
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " probe",
                PreviewCommand.build(PreviewCommand.Action.PROBE)
        );
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " enroll " + TargetProfile.PACKAGE,
                PreviewCommand.build(PreviewCommand.Action.ENROLL)
        );
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " status " + TargetProfile.PACKAGE,
                PreviewCommand.build(PreviewCommand.Action.STATUS)
        );
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " switch " + TargetProfile.PACKAGE
                        + " " + TargetProfile.PREVIEW_SLOT,
                PreviewCommand.build(PreviewCommand.Action.SWITCH_PREVIEW)
        );
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " switch " + TargetProfile.PACKAGE
                        + " " + TargetProfile.BASE_SLOT,
                PreviewCommand.build(PreviewCommand.Action.SWITCH_BASE)
        );
        assertEquals(
                PreviewCommand.SLOTCTL_PATH + " rescue " + TargetProfile.PACKAGE + " --to-base",
                PreviewCommand.build(PreviewCommand.Action.RESCUE_BASE)
        );
    }

    @Test
    public void generatedProfileContainsOnlyFixedTargets() {
        String expectedPackage = switch (TargetProfile.PROFILE) {
            case "slotprobe" -> "com.uclone.slotprobe";
            case "fitness" -> "com.asksky.fitness";
            default -> throw new AssertionError("unexpected generated target profile");
        };
        assertEquals(expectedPackage, TargetProfile.PACKAGE);
        assertEquals(0, TargetProfile.USER_ID);
        assertEquals("base", TargetProfile.BASE_SLOT);
        assertEquals("preview", TargetProfile.PREVIEW_SLOT);
        assertEquals("uclone-slices-preview", TargetProfile.MODULE);
    }

    @Test
    public void exposesNoArbitraryPackageOrSlotBuilder() throws Exception {
        int buildMethods = 0;
        for (Method method : PreviewCommand.class.getDeclaredMethods()) {
            if (method.getName().equals("build")) {
                buildMethods += 1;
            }
        }
        assertEquals(1, buildMethods);
        PreviewCommand.class.getDeclaredMethod("build", PreviewCommand.Action.class);
    }

    @Test
    public void commandSurfaceContainsNoShellExpansionCharacters() {
        for (PreviewCommand.Action action : PreviewCommand.Action.values()) {
            String command = PreviewCommand.build(action);
            assertFalse(command.contains(";"));
            assertFalse(command.contains("|"));
            assertFalse(command.contains("$"));
            assertFalse(command.contains("`"));
            assertFalse(command.contains(".."));
        }
    }

    @Test
    public void mutationsRequireConfirmation() {
        assertFalse(PreviewCommand.Action.PROBE.confirmationRequired());
        assertFalse(PreviewCommand.Action.STATUS.confirmationRequired());
        assertEquals(true, PreviewCommand.Action.ENROLL.confirmationRequired());
        assertEquals(true, PreviewCommand.Action.SWITCH_PREVIEW.confirmationRequired());
        assertEquals(true, PreviewCommand.Action.SWITCH_BASE.confirmationRequired());
        assertEquals(true, PreviewCommand.Action.RESCUE_BASE.confirmationRequired());
    }

    @Test
    public void actionsUseFixedDemoSafetyTimeouts() {
        assertEquals(30_000L, PreviewCommand.Action.PROBE.timeoutMillis());
        assertEquals(120_000L, PreviewCommand.Action.ENROLL.timeoutMillis());
        assertEquals(30_000L, PreviewCommand.Action.STATUS.timeoutMillis());
        assertEquals(1_800_000L, PreviewCommand.Action.SWITCH_PREVIEW.timeoutMillis());
        assertEquals(120_000L, PreviewCommand.Action.SWITCH_BASE.timeoutMillis());
        assertEquals(600_000L, PreviewCommand.Action.RESCUE_BASE.timeoutMillis());
    }
}
