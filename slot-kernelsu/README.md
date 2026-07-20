# UClone Slots Preview KernelSU skeleton

This directory is a non-installed preview module skeleton. It is not part of
the Android APK build and no script here should be run against a device until
the Rust runtime and rescue procedure have been reviewed for that device.

## Packaging layout

```text
slot-kernelsu/
├── module.prop
├── skip_mount
├── post-fs-data.sh
├── emergency-containment.sh
├── startup-gate.sh
├── service.sh
├── boot-completed.sh
├── boot-state.sh
├── rescue.sh
├── package.sh       # fixed-path wrapper; accepts no artifact arguments
└── bin/
    ├── ucloned       # optional packaged Rust daemon, executable at package time
    └── slotctl       # optional packaged Rust client, executable at package time
```

`bin/` is intentionally empty in this source preview. The fixed packaging
step copies project-produced `ucloned` and `slotctl` binaries, plus these
runtime artifacts:

```text
runtime/slot-bridge.apk
runtime/slot-fsprobe
```

At boot KernelSU exposes those helpers from the fixed module-id directory:

```text
/data/adb/modules/uclone-slices-preview/runtime/slot-bridge.apk
/data/adb/modules/uclone-slices-preview/runtime/slot-fsprobe
```

The Rust runtime uses those immutable module paths directly. It does not copy
executables into the mutable control-plane root.

Packaged hooks and native executables are root-only `0700`; the bridge APK is
root-only `0600`. `module.prop` and `skip_mount` remain conventional `0644`
KernelSU metadata. The packager extracts its own ZIP and verifies these modes
before publishing checksums.

`package.sh` accepts no source, destination, or artifact arguments and delegates
to `tools/package-kernelsu-preview.sh`. The canonical output is written under
`.omo/evidence/slices-preview-package/`, together with the fixed ZIP entry list,
manifest, package summary, and SHA-256 records. The fsprobe input is always the
NDK arm64-v8a output at:

```text
slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
```

From that evidence directory, `shasum -a 256 -c SHA256SUMS` independently
checks the ZIP and every staged module artifact.

## Fixed runtime contract

The module owns only the control plane below:

```text
/data/adb/uclone-slices-preview/
├── logs/
├── run/
├── state/
├── journal/
├── rescue-journal/
├── registry/
├── enrollment/
├── enrollment-attempts/
├── package-state/
└── catalog/
```

Long-lived CE and DE slot data must remain in the user encryption domains:

```text
/data/misc_ce/0/uclone-slices-preview/slots/
/data/misc_de/0/uclone-slices-preview/slots/
```

The early boot hooks do not create, copy, delete, or relabel app data. A
user-triggered `slotctl enroll` or `slotctl switch` may operate only on the
fixed, allowlisted Preview package and its derived CE/DE slot paths; those
operations are journaled and fail closed. The module still does not install an
APK, add an OverlayFS mount, add `sepolicy.rule`, or alter system partitions.
`skip_mount` is present specifically so KernelSU does not apply module
overlays.

## Hook safety

- `post-fs-data.sh` creates fixed `0700` control-plane directories, publishes a
  boot-scoped pending marker, starts `startup-gate.sh` in the background, and
  returns immediately. If a required helper or control path is unsafe, it
  starts the fixed-package emergency containment worker and still does not
  block boot. It never reads CE application data.
- `emergency-containment.sh` recognizes only durable SlotProbe management
  anchors. It repeatedly disables and force-stops only
  `com.uclone.slotprobe`, and returns success only after both disabled state
  and process quiescence are proved. It never publishes daemon readiness.
- `startup-gate.sh` repeatedly invokes fixed `bin/ucloned --startup-gate` in
  PID 1's mount namespace. That Rust path captures or reuses the exact
  enabled/suspended lease before disabling the allowlisted package. Only a
  proved held gate (or proved absence of management artifacts) publishes the
  current-boot readiness marker.
- `service.sh` refuses ordinary daemon startup until that current-boot marker
  is proved. The daemon reasserts the exact-lease gate before opening ordinary
  stores; an abnormal exit withholds restart until that gate succeeds again.
  Normal operation never performs an unleased raw `disable-user`; the sole
  exception is the fail-closed emergency worker used when the exact Rust lease
  runtime is unavailable. That worker cannot restore enabled state.
- `boot-completed.sh` persists a reconcile request and retries bounded
  `slotctl reconcile` attempts in the background. `locked` and `held` retain
  the marker and back off up to 30 seconds; only a typed non-locked terminal
  result consumes it. `service.sh` starts a second fixed worker so one killed
  reconcile command does not lose the durable retry request.

The post-fs background worker is not yet proof that containment beats Android's
`LOCKED_BOOT_COMPLETED` delivery. SlotProbe has a direct-boot-aware receiver, so
the ordering between PackageManager readiness, exact lease publication, package
disable, and receiver dispatch remains a mandatory real-device blocker. The
module must not be treated as safe for a real app until timestamped DE/process
evidence proves that ordering on the target HyperOS build.
- `rescue.sh` accepts only `com.uclone.slotprobe` and an explicit `--to-base`.
  It delegates complete base rescue to `slotctl` in PID 1's mount namespace
  through validated `toybox nsenter`. Its fail-closed fallback verifies the
  disable and quiesce commands before saying containment was proved; command
  failures are reported as containment-unproven rather than claiming the
  package remains disabled.
  The Rust path opens only the exact immutable enrollment, base catalog,
  rescue journal, and canonical gate lease. It retires Preview CE/DE mounts,
  proves native base across canonical, mirror, and Zygote views, then proves
  exact enabled and suspended state before returning success. Corrupt anchors,
  identity drift, an unknown gate state, or an unavailable runtime keep the
  package disabled and report `RECOVERY_REQUIRED`; rescue never guesses a state
  and never calls an unconditional `enable`.

## Local validation

Run from the repository root:

```bash
for script in slot-kernelsu/*.sh; do
  sh -n "$script"
done
test -f slot-kernelsu/skip_mount
test ! -e slot-kernelsu/sepolicy.rule
```

Building a KernelSU ZIP and installing it on a phone are separate, explicitly
authorized steps. This source change performs neither operation.
