# Slot Bridge

`slot-bridge` is a Java-only Android 16/API 36 bridge loaded directly by
`app_process`. The APK is a DEX container; it is never installed and declares
no Android components or permissions.

## Build and host tests

The module can build independently, so the repository root does not need to
include it in `settings.gradle.kts`:

```sh
JAVA_HOME=/usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
ANDROID_HOME=/usr/local/share/android-commandlinetools \
gradle --no-daemon -p slot-bridge testReleaseUnitTest appProcessArtifact
```

The fixed runtime artifact is:

```text
slot-bridge/build/app-process/slot-bridge.apk
```

It contains `classes.dex` and the entry point
`com.uclone.slotbridge.Main.main(String[])`.

## Fixed invocation

The trusted root runtime stages the artifact at the fixed classpath before
using one of these exact invocations:

```sh
CLASSPATH=/data/adb/uclone-slices-preview/runtime/slot-bridge.apk \
/system/bin/app_process /system/bin com.uclone.slotbridge.Main probe-device

CLASSPATH=/data/adb/uclone-slices-preview/runtime/slot-bridge.apk \
/system/bin/app_process /system/bin com.uclone.slotbridge.Main \
probe-package com.uclone.slotprobe

CLASSPATH=/data/adb/uclone-slices-preview/runtime/slot-bridge.apk \
/system/bin/app_process /system/bin com.uclone.slotbridge.Main \
set-enabled com.uclone.slotprobe disabled_user

CLASSPATH=/data/adb/uclone-slices-preview/runtime/slot-bridge.apk \
/system/bin/app_process /system/bin com.uclone.slotbridge.Main \
set-suspended com.uclone.slotprobe true
```

The enabled enum is exactly one of `default`, `enabled`, `disabled`,
`disabled_user`, or `disabled_until_used`. Suspension accepts only lowercase
`true` or `false`.

These commands are documentation for the root runtime contract. Host
validation does not execute them, install an APK, contact a device, or perform
ADB writes.

## Boundary

- Android user is fixed to user 0.
- Package is fixed to `com.uclone.slotprobe`.
- No caller path, flag map, shell fragment, environment-derived package, or
  alternate command is accepted.
- CE and DE paths are computed as `/data/user/0/<package>` and
  `/data/user_de/0/<package>`, then compared with PackageManager's user-0
  `ApplicationInfo` paths.
- Package facts and mutations use the API 36 PackageManager and UserManager
  Binder interfaces through `ServiceManager`; there is no `dumpsys`, process
  execution, network access, or caller-selected filesystem traversal.
- API 36 does not expose PackageManager's persisted CE/DE inode values through
  `IPackageManager`. After a Binder flush, the bridge therefore reads only the
  fixed `/data/system/users/0/package-restrictions.xml`. The public Android
  `Os` API has no `openat` or `O_DIRECTORY`, so the bridge uses fixed absolute
  paths, `O_NOFOLLOW` on every open, no-follow `lstat` checks for every parent,
  trusted root/system directory ownership, and descriptor revalidation after the
  read. The final regular file must be UID/GID 1000 with one link and a 4 MiB
  cap. No XML path is accepted from argv; a privileged root attacker racing
  parent replacement is outside the app_process bridge trust boundary.
- The XML parser requires the Android 16 `package-restrictions` root and exactly
  one direct `pkg` node for `com.uclone.slotprobe`, rejects DTD/entities and
  malformed UTF-8, and requires nonzero decimal `ceDataInode` and `deDataInode`.
  These are emitted as `packageManagerCeInode` and
  `packageManagerDeInode`; every mismatch fails closed.
- Enabled mutations set the API 36 `PackageManager.SYNCHRONOUS` value `0x2`.
  Suspension has no equivalent flag, so the bridge calls
  `flushPackageRestrictionsAsUser(0)` before reading state back.
- Every invocation writes exactly one UTF-8 JSON object followed by one LF.
  The complete frame is capped at 16,384 bytes. Failures contain only a fixed
  request ID and stable error code; exception text, stack traces, and paths are
  never emitted.

Current signer identity uses only `SigningInfo.getApkContentsSigners()`. A
single signer is its lowercase certificate SHA-256. Multiple current signers
are individually hashed, unsigned-byte sorted, and hashed again with the
domain `uclone-current-signers-v1`, a NUL byte, a two-byte signer count, and the
ordered 32-byte digests. Signing history is intentionally excluded.
