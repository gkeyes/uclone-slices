package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class PackagePolicyTest {
    @Test
    public void computesCanonicalUserZeroPathsWhenPackageIsAllowlisted() throws BridgeFailure {
        String packageName = PackagePolicy.ALLOWED_PACKAGE;

        PackagePaths paths = PackagePolicy.pathsFor(packageName);

        assertEquals(TargetProfile.TARGET_CE, paths.ceDataPath());
        assertEquals(TargetProfile.TARGET_DE, paths.deDataPath());
    }

    @Test
    public void allowlistRequiresExactPackageSpelling() {
        assertTrue(PackagePolicy.isAllowed(PackagePolicy.ALLOWED_PACKAGE));
        assertFalse(PackagePolicy.isAllowed(PackagePolicy.ALLOWED_PACKAGE + ".beta"));
        assertFalse(PackagePolicy.isAllowed(PackagePolicy.ALLOWED_PACKAGE.toUpperCase()));
        assertFalse(PackagePolicy.isAllowed(PackagePolicy.ALLOWED_PACKAGE + "/../other"));
    }
}
