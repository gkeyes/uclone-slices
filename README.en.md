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

Roadmap: optional Launcher / LSPosed entry (not connected to Preview today)
  └── future shortcut routing only; no root or data operations
```

The KernelSU Runtime is required. LSPosed is not required. The existing
`launcher-module/` currently targets Legacy UClone Restore and does not provide
a Preview slot entry.

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

Ordinary third-party apps remain supported by default. A third-party app that declares Direct Boot is now classified as conditionally supported after user0 unlock instead of being rejected outright. The manager shows a dedicated warning and requires explicit acceptance; the runtime binds that acceptance to the package UID, signature, and support class. This does not claim pre-unlock or active-slot reboot support. System and shared-UID apps remain blocked. See [Direct Boot conditional support](docs/DIRECT_BOOT_CONDITIONAL_SUPPORT.md).

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
| `slot-probe/` | Diagnostics: dedicated CE/DE and multi-process regression app |
| `slot-preview-controller/` | Diagnostics: fixed-target controller retained for regression |
| `docs/` | design notes, device evidence, and safety boundaries |
| `app/`, `launcher-module/` | Legacy UClone Restore and its Launcher/LSPosed entry |

See [Product and module boundaries](docs/architecture/PRODUCT_BOUNDARIES.md) for
the current separation and roadmap.

## Build and validation

This repository combines Android, Rust, C, and KernelSU shell components. Release artifacts must come from one source commit and pass component tests, strict Clippy, Android lint, ELF/signing checks, and KernelSU ZIP content review.

The paired Preview release contains `slot-manager-app/` plus the Runtime built
from `slot-runtime/`, `slot-bridge/`, `slot-fsprobe/`, and `slot-kernelsu/`.
Diagnostic modules and the Legacy `app/` and `launcher-module/` use separate
build targets and are not included in the Preview APK/ZIP.

Release builds run only in the repository's pinned GitHub Actions environment.
Local work is limited to static source, shell/YAML syntax, formatting, and path
policy checks; local caches and toolchains are not release evidence. CI runs:

```bash
Rust fmt, tests, strict Clippy, and rustdoc
Android unit tests, lint, and release assembly
Bridge, fsprobe, KernelSU boot, containment, and rescue contract tests
```

The CLI requires explicit consent when enrolling a Direct Boot package:

```bash
slotctl enroll com.example.app --accept-direct-boot-conditional
```

The KernelSU ZIP is produced by the fixed-path packaging workflow. The source skeleton itself is not an installable release artifact.

The GitHub Action on `main` builds a paired fixed-signed
`uclone-slots-preview.apk` and generic `uclone-slices-preview-kernelsu.zip`.
The signing job receives only the validated unsigned bundle; the publishing job
cannot read signing secrets. A draft Release is populated and its exact asset
set is verified before publication. Protocol v2 requires the APK and Runtime to
have the same `build_id`, so mixed builds fail closed. Before a paired upgrade,
all managed apps must be rescued to Base and active gate/management state must
be retired. Migrating from the legacy v1/Fitness control plane also requires
stopping the old daemon and renaming its control-plane directory into a
reversible archive after native Base is proved; v1 state must not be overwritten
in place. Moving from a debug-signed APK requires one uninstall; later builds
with the fixed identity can update in place. The private key exists only in
GitHub Actions Secrets and a protected local backup. The repository stores only
the public certificate and fingerprint.

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
