package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class PackagePolicyTest {
    private static final String PACKAGE = "com.example.valid_2";

    @Test
    public void computesCanonicalUserZeroPathsForValidatedPackage() throws BridgeFailure {
        PackagePaths paths = PackagePolicy.pathsFor(PACKAGE);

        assertEquals("/data/user/0/" + PACKAGE, paths.ceDataPath());
        assertEquals("/data/user_de/0/" + PACKAGE, paths.deDataPath());
    }

    @Test
    public void acceptsJavaStylePackageAndRejectsPathOrShellSyntax() {
        assertTrue(PackagePolicy.isAllowed(PACKAGE));
        assertTrue(PackagePolicy.isAllowed("Com.Example.App"));
        assertFalse(PackagePolicy.isAllowed("single"));
        assertFalse(PackagePolicy.isAllowed("com.2bad.app"));
        assertFalse(PackagePolicy.isAllowed(PACKAGE + "/../other"));
        assertFalse(PackagePolicy.isAllowed(PACKAGE + ";id"));
    }
}
