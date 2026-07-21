# UClone Slices Preview

UClone Slices Preview is an experimental multi-slot data system for rooted Android devices. One installed APK can keep a native Base slot and additional persistent CE/DE slots. Switching changes the mounted data view instead of repeatedly restoring a full backup.

> Status: Preview. The mount path and a Fitness A/B proof of concept have been validated on Android 16, HyperOS 3, and KernelSU. The first generic multi-app runtime and manager APK are implemented, but cross-device, reboot, and full package-lifecycle validation are not complete. Do not use this project with important app data.

Chinese: [README.md](README.md)

## Architecture

```text
UClone Slots Preview APK
  └── app selection, slot management, health, and recovery UI

KernelSU Runtime
  ├── ucloned: root transaction and mount service
  ├── slotctl: constrained management and rescue client
  └── boot hooks: containment, recovery, and reconciliation

Optional Launcher / LSPosed entry
  └── shortcut routing only; no root or data operations
```

The KernelSU Runtime is required. LSPosed is optional.

## Validated foundation

- A Global Propagated Bind backend synchronized CE/DE, `/data/data`, `/data_mirror`, Zygote, and new app-process views on the target HyperOS device.
- Base remains the native Android data directory and inode; it is never moved or overwritten.
- Extra slots live in the matching user0 CE and DE encryption domains.
- A switch transaction covers the Journal, app execution gate, process quiescence, CE/DE mounts, view verification, Registry commit, and exact gate restoration.
- Unknown identity, transaction, or view state fails closed: the target app remains disabled and enters `RecoveryRequired`.
- The Preview does not modify system partitions, use OverlayFS, or add permissive SELinux rules.

See [the device feasibility report](docs/SLICES_PREVIEW_DEVICE_FEASIBILITY.md) for the current evidence.

## Current scope

The first phase supports only:

- user0;
- ordinary third-party apps;
- paired CE + DE switching;
- one active slot at a time;
- SELinux Enforcing;
- devices whose KernelSU mount-master topology passes the runtime probe.

File slots do not fully isolate Android Keystore, AccountManager, permissions, AppOps, notifications, jobs/alarms, external storage, or server-side device state.

Managed-app updates, clear-data, uninstall/reinstall, and OTA flows require lifecycle guards. Automatic updates should remain disabled for Preview targets until those paths are validated.

## Repository layout

| Path | Purpose |
| --- | --- |
| `slot-runtime/` | Rust runtime, transactions, Registry, Journal, and CLI |
| `slot-manager-app/` | Generic Kotlin + Jetpack Compose manager APK |
| `slot-bridge/` | PackageManager identity and state bridge |
| `slot-fsprobe/` | Native filesystem, inode, and mount-view probe |
| `slot-kernelsu/` | KernelSU runtime module and independent rescue scripts |
| `slot-probe/` | Dedicated CE/DE and multi-process regression app |
| `slot-preview-controller/` | Fixed-target diagnostic controller retained for regression |
| `docs/` | design notes, device evidence, and safety boundaries |
| `app/`, `launcher-module/` | upstream UClone Restore and optional shortcut baseline |

## Build and validation

This repository combines Android, Rust, C, and KernelSU shell components. Release artifacts must come from one source commit and pass component tests, strict Clippy, Android lint, ELF/signing checks, and KernelSU ZIP content review.

Typical local checks:

```bash
cargo test --manifest-path slot-runtime/Cargo.toml
cargo clippy --manifest-path slot-runtime/Cargo.toml --all-targets --all-features -- -D warnings
./gradlew :slot-manager-app:testDebugUnitTest :slot-manager-app:lintDebug :slot-manager-app:assembleDebug
```

The KernelSU ZIP is produced by the fixed-path packaging workflow. The source skeleton itself is not an installable release artifact.

## Safety contract

The only runnable terminal states are:

```text
complete Base
complete target slot
disabled app in RecoveryRequired
```

An unknown state must never cause the target app to be enabled or launched automatically. Reboots, real-app enrollment, and module installation require a passing device probe, a verified rescue path, and explicit user authorization.

## Repository role

This repository develops UClone Slices Preview independently. It is not a stable UClone Restore `0.3.x` upgrade. Once validated, the APK experience may become UClone's “Data Spaces” feature while the root runtime remains a separate KernelSU component.
