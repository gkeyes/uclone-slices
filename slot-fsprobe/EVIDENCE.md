# Verification evidence

This file records source-only verification for the native helper on 2026-07-18. No device command,
installation, or on-device execution was performed.

## Passed host gate

```text
$ make -C slot-fsprobe test
cc -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic -Wshadow -Wformat=2
  -Wstrict-prototypes -Wmissing-prototypes -fstack-protector-strong ...
./build/host/slot-fsprobe-tests
PASS: SHA-256 vectors and fixed-path validation
```

The test covers the empty, `abc`, multi-block, and one-million-`a` published SHA-256 vectors. It
also covers accepted canonical/slot/staging paths and rejection of aliases, other users/packages,
traversal, empty segments, descendants, malformed slot identifiers, and overlong paths.

All C/header/test source files are below 250 lines.

## Passed Android arm64 gate

The NDK 29 arm64-only build completed without compiler warnings or errors:

```text
$ ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/29.0.14206865" \
    make -C slot-fsprobe android-check
[arm64-v8a] Executable     : slot-fsprobe
[arm64-v8a] Install        : slot-fsprobe => build/android/bin/arm64-v8a/slot-fsprobe
```

The resulting artifact is a regular executable (not a symlink), 9,008 bytes, AArch64 ELF PIE,
with GNU_RELRO, GNU_STACK, BIND_NOW/NOW, and `__stack_chk_fail` present:

```bash
file slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
# ELF 64-bit LSB pie executable, ARM aarch64, ... interpreter /system/bin/linker64

/usr/local/share/android-commandlinetools/ndk/29.0.14206865/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-readelf \
  --program-headers slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
# GNU_RELRO; GNU_STACK RW

/usr/local/share/android-commandlinetools/ndk/29.0.14206865/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-readelf \
  --dynamic slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
# BIND_NOW; NOW PIE

shasum -a 256 slot-fsprobe/build/android/bin/arm64-v8a/slot-fsprobe
# 28e27169df16b34ad77332f967c988e710a9c72f43580a0eaf139172a749c9df
```

This is a compile/ELF gate only. No device command, installation, or on-device execution was
performed. Device-only gaps remain: verifying the ioctl against the HyperOS CE/DE policies and
checking the runtime `getfattr` SELinux value. The latter is intentionally owned by the Rust
materializer; this helper only gates directory `stat(2)` metadata and returns the fscrypt digest.
