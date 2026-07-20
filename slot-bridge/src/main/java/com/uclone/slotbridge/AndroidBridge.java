package com.uclone.slotbridge;

import android.os.Build;

final class AndroidBridge {
    private final BinderServices services;

    private AndroidBridge(BinderServices services) {
        this.services = services;
    }

    static AndroidBridge connect() throws BridgeFailure {
        return new AndroidBridge(BinderServices.connect());
    }

    DeviceSnapshot probeDevice() throws BridgeFailure {
        Object value = BinderInvoke.call(
                services.userManager(),
                "device",
                ErrorCode.INTERNAL,
                "isUserUnlocked",
                new Class<?>[] {int.class},
                PackagePolicy.ALLOWED_USER_ID);
        if (!(value instanceof Boolean)) {
            throw new BridgeFailure("device", ErrorCode.INVALID_RESPONSE);
        }
        return new DeviceSnapshot((Boolean) value, Build.VERSION.SDK_INT);
    }

    PackageSnapshot probePackage(String packageName) throws BridgeFailure {
        return reader(packageName).read();
    }

    GateSnapshot probeGate(String packageName) throws BridgeFailure {
        return reader(packageName).readGate();
    }

    void setEnabled(String packageName, EnabledState state) throws BridgeFailure {
        PackageReader reader = reader(packageName);
        new PackageMutator(services.packageManager(), reader).setEnabled(state);
    }

    void setSuspended(String packageName, boolean suspended) throws BridgeFailure {
        PackageReader reader = reader(packageName);
        new PackageMutator(services.packageManager(), reader).setSuspended(suspended);
    }

    private PackageReader reader(String packageName) throws BridgeFailure {
        if (!PackagePolicy.isAllowed(packageName)) {
            throw new BridgeFailure("package", ErrorCode.PACKAGE_NOT_ALLOWED);
        }
        return new PackageReader(services.packageManager(), packageName);
    }
}
