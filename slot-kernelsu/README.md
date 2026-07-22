# UClone Slots Preview KernelSU Runtime

This directory is the source skeleton for the generic multi-app Runtime. It is
not itself an installable artifact. Only the paired ZIP produced by the same
GitHub Actions run as the manager APK is eligible for installation.

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
    ├── ucloned       # packaged Rust daemon
    └── slotctl       # packaged Rust client and direct Base rescue entry
```

`bin/` is intentionally empty in this source preview. The fixed packaging
step copies project-produced `ucloned` and `slotctl` binaries, plus these
runtime artifacts:

```text
runtime/slot-bridge.apk
runtime/slot-fsprobe
```

The ZIP also contains a read-only generated target profile and the validated
`profile-loader.sh`. A missing, mutable, symlinked, or unknown profile falls
back to generic discovery; boot hooks never source unvalidated target content.

At boot KernelSU exposes helpers from the fixed module-id directory:

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
├── compatibility-policy/
├── enrollment-attempts/
├── package-state/
├── slot-metadata/
└── catalog/
```

Long-lived CE and DE slot data must remain in the user encryption domains:

```text
/data/misc_ce/0/uclone-slices-preview/slots/
/data/misc_de/0/uclone-slices-preview/slots/
```

The early boot hooks do not create, copy, delete, or relabel app data. A
user-triggered request may address only a syntactically valid user0 package
that passes Runtime inspection or already has durable management evidence.
Every CE/DE and slot path is derived internally from that package and an
immutable slot id; no public command accepts arbitrary paths or shell text.
The module does not install an APK, add OverlayFS, add `sepolicy.rule`, or alter
system partitions. `skip_mount` prevents KernelSU module overlays.

## Hook safety

- `post-fs-data.sh` creates fixed `0700` control-plane directories, publishes a
  boot-scoped pending marker, starts `startup-gate.sh` in the background, and
  returns immediately. If a helper is absent or unsafe, a built-in persistent
  worker discovers packages only from bounded management roots, repeatedly
  disables and force-stops them, and still never blocks Android boot or reads
  CE application data.
- `emergency-containment.sh` uses the same bounded generic discovery. It exits
  only after no enrollment, policy, registry, slot metadata, transaction,
  rescue, Gate, or DE slot evidence remains; re-enabled or restarted Apps are
  contained again while evidence exists.
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

The post-fs background worker is not proof that containment beats every OEM
`LOCKED_BOOT_COMPLETED` race. A Direct Boot conditional package committed to a
non-Base slot is therefore not released automatically after reboot: Runtime
keeps its Gate and returns `RecoveryRequired(ConditionalDirectBoot)` until an
unlocked reconciliation or Base rescue proves a safe view. A real reboot still
requires explicit user confirmation and timestamped device evidence.

- `rescue.sh` accepts one validated package and the exact `--to-base` action.
  It delegates complete Base rescue to `slotctl` in PID 1's mount namespace.
  If the daemon socket is unavailable, protocol v2 permits only this already
  decoded rescue command to enter the fixed direct path. A fallback must prove
  disabled state and zero target processes; command failure is reported as
  containment-unproven, never as rescue success.
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

The packaged ZIP contains a zero-length `disable` marker and is disabled by
default. Paired upgrade is refused while any managed App metadata or Gate lease
exists; rescue all Apps to Base first. The v2 boot root uses control-plane
version `2`. A legacy v1/Fitness root must be stopped and renamed into a
reversible archive only after native Base is proved; the module never overwrites
or guesses how to migrate old state. Building, installing, enabling, and
rebooting are separate actions. Reboot is never implied by source or CI work.
