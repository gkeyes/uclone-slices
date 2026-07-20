package com.uclone.slotbridge;

import android.os.Build;

final class AndroidBridge {
    private final BinderServices services;
    private final PackageReader reader;
    private final PackageMutator mutator;

    private AndroidBridge(BinderServices services) {
        this.services = services;
        this.reader = new PackageReader(services.packageManager());
        this.mutator = new PackageMutator(services.packageManager(), reader);
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

    PackageSnapshot probePackage() throws BridgeFailure {
        return reader.read();
    }

    GateSnapshot probeGate() throws BridgeFailure {
        return reader.readGate();
    }

    void setEnabled(EnabledState state) throws BridgeFailure {
        mutator.setEnabled(state);
    }

    void setSuspended(boolean suspended) throws BridgeFailure {
        mutator.setSuspended(suspended);
    }
}
