# Slices Preview local gates

These scripts are host-only validation and packaging entry points for the
`slices-preview` branch. They use only fixed paths derived from this checkout,
accept no command or path arguments, and never run ADB, `su`, install, reboot,
mount, or any other device operation.

## Validation

From the repository root:

```bash
tools/validate-slices-preview.sh
```

The script runs, in order:

1. the 250-line production-source gate;
2. Rust format, check, test, Clippy, and rustdoc gates;
3. a fixed NDK 29 Android aarch64 Rust release build;
4. host and arm64-v8a `slot-fsprobe` builds/tests;
5. standalone `slot-bridge` unit tests, Release lint, and `app_process` artifact build;
6. `slot-probe` JVM tests, debug APK, instrumentation APK, and lint.

Logs and the fixed artifact summary are written to:

```text
.omo/evidence/slices-preview-validation/
```

The validation task is intentionally separate from device QA. A successful
host run does not claim that the KernelSU module has been installed or that a
phone has been rebooted.

The Android Rust step can also be run alone when only the fixed cross-build is
needed:

```bash
tools/build-slot-runtime-android.sh
```

It accepts no target, linker, output, or artifact arguments and writes its
build log to `.omo/evidence/slices-preview-validation/rust-android-aarch64.log`.

## KernelSU preview package

After the host gates have built the cross-compiled Rust binaries and helper
artifacts, run:

```bash
tools/package-kernelsu-preview.sh
```

The packager only copies these fixed artifacts:

```text
slot-runtime/target/aarch64-linux-android/release/ucloned
slot-runtime/target/aarch64-linux-android/release/slotctl
slot-bridge/build/app-process/slot-bridge.apk
slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
```

The `slot-probe` APKs remain validation-only artifacts and are deliberately
not embedded in the KernelSU module.

It stages them with the fixed `slot-kernelsu` hooks, verifies the exact ZIP
entry list and every staged-file checksum, emits a ZIP, and writes a manifest,
entry list, package summary, and SHA-256 record under:

```text
.omo/evidence/slices-preview-package/
```

Missing, symlinked, non-executable, or unexpected `sepolicy.rule` artifacts
abort packaging. `slot-kernelsu/package.sh` is only a no-argument convenience
wrapper for the same fixed-path packager. Packaging does not compile, install,
or touch a device. The emitted `SHA256SUMS` is rooted at
`.omo/evidence/slices-preview-package/`, so it can be independently checked
with `shasum -a 256 -c SHA256SUMS` from that directory.
