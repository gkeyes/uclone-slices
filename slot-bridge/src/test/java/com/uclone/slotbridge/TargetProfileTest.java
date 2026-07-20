package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class TargetProfileTest {
    @Test
    public void generatedConstantsMatchTheSelectedFixedProfile() throws BridgeFailure {
        String expected = switch (TargetProfile.PROFILE) {
            case "slotprobe" -> "com.uclone.slotprobe";
            case "fitness" -> "com.asksky.fitness";
            case "generic" -> "com.uclone.slots.preview";
            default -> throw new AssertionError("unknown target profile");
        };
        assertEquals(expected, TargetProfile.PACKAGE);
        assertEquals(0, TargetProfile.USER_ID);
        assertEquals(0, PackagePolicy.ALLOWED_USER_ID);
        assertEquals(TargetProfile.USER_ID, PackagePolicy.ALLOWED_USER_ID);
        PackagePaths paths = PackagePolicy.pathsFor(TargetProfile.PACKAGE);
        assertEquals(TargetProfile.TARGET_CE, paths.ceDataPath());
        assertEquals(TargetProfile.TARGET_DE, paths.deDataPath());
    }

    @Test
    public void bridgeAcceptsOtherWellFormedPackageWithoutRebuild() {
        String foreign = "com.example.other";

        assertTrue(PackagePolicy.isAllowed(TargetProfile.PACKAGE));
        assertTrue(PackagePolicy.isAllowed(foreign));
    }
}
