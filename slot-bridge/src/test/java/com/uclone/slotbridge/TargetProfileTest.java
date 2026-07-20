package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class TargetProfileTest {
    @Test
    public void generatedConstantsMatchTheSelectedFixedProfile() throws BridgeFailure {
        if ("slotprobe".equals(TargetProfile.PROFILE)) {
            assertEquals("com.uclone.slotprobe", TargetProfile.PACKAGE);
        } else {
            assertEquals("fitness", TargetProfile.PROFILE);
            assertEquals("com.asksky.fitness", TargetProfile.PACKAGE);
        }
        assertEquals(0, TargetProfile.USER_ID);
        assertEquals(TargetProfile.PACKAGE, PackagePolicy.ALLOWED_PACKAGE);
        assertEquals(TargetProfile.USER_ID, PackagePolicy.ALLOWED_USER_ID);
        PackagePaths paths = PackagePolicy.pathsFor(TargetProfile.PACKAGE);
        assertEquals(TargetProfile.TARGET_CE, paths.ceDataPath());
        assertEquals(TargetProfile.TARGET_DE, paths.deDataPath());
    }

    @Test
    public void bridgeRejectsTheOtherWellFormedProfilePackage() {
        String foreign = "slotprobe".equals(TargetProfile.PROFILE)
                ? "com.asksky.fitness"
                : "com.uclone.slotprobe";

        assertTrue(PackagePolicy.isAllowed(TargetProfile.PACKAGE));
        assertFalse(PackagePolicy.isAllowed(foreign));
    }
}
