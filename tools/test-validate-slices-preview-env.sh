#!/usr/bin/env bash

set -eu
set -o pipefail

SCRIPT_UNDER_TEST=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/validate-slices-preview.sh
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/slices-preview-env.XXXXXX")
trap 'rm -rf -- "$TEST_ROOT"' EXIT
TEST_ROOT=$(CDPATH= cd -- "$TEST_ROOT" && pwd -P)

FAKE_BIN="$TEST_ROOT/fake-bin"
JAVA_HOME_FIXTURE="$TEST_ROOT/java-home"
ANDROID_HOME_FIXTURE="$TEST_ROOT/home/Library/Android/sdk"
NDK_HOME_FIXTURE="$ANDROID_HOME_FIXTURE/ndk/29.0.14206865"
CONFLICT_NDK_FIXTURE="$TEST_ROOT/conflicting-ndk"
VALIDATOR="$TEST_ROOT/tools/validate-slices-preview.sh"
BUILD_SCRIPT="$TEST_ROOT/tools/build-slot-runtime-android.sh"
mkdir -p "$FAKE_BIN" "$JAVA_HOME_FIXTURE/bin" "$NDK_HOME_FIXTURE" "$CONFLICT_NDK_FIXTURE" "$TEST_ROOT/tools" "$TEST_ROOT/slot-runtime/.cargo"

cp "$SCRIPT_UNDER_TEST" "$VALIDATOR"
chmod +x "$VALIDATOR"
cp "$(dirname -- "$SCRIPT_UNDER_TEST")/build-slot-runtime-android.sh" "$BUILD_SCRIPT"
chmod +x "$BUILD_SCRIPT"
printf '%s\n' '[target.aarch64-linux-android]' >"$TEST_ROOT/slot-runtime/.cargo/config.toml"
printf '%s\n' 'Pkg.Revision = 29.0.14206865' >"$NDK_HOME_FIXTURE/source.properties"
for helper in \
    test-kernelsu-boot-flow.sh \
    test-kernelsu-upgrade-gate.sh \
    test-kernelsu-upgrade-proof.sh; do
    : >"$TEST_ROOT/tools/$helper"
    chmod +x "$TEST_ROOT/tools/$helper"
done
for tool in cargo gradle make; do
    : >"$FAKE_BIN/$tool"
    chmod +x "$FAKE_BIN/$tool"
done
: >"$JAVA_HOME_FIXTURE/bin/java"
chmod +x "$JAVA_HOME_FIXTURE/bin/java"
case "$(uname -s):$(uname -m)" in
    Darwin:arm64) PREBUILT_HOST=darwin-arm64 ;;
    Darwin:x86_64) PREBUILT_HOST=darwin-x86_64 ;;
    Linux:x86_64) PREBUILT_HOST=linux-x86_64 ;;
    *) printf '%s\n' 'unsupported fixture host' >&2; exit 1 ;;
esac
LINKER_FIXTURE="$NDK_HOME_FIXTURE/toolchains/llvm/prebuilt/$PREBUILT_HOST/bin/aarch64-linux-android29-clang"
mkdir -p "$(dirname -- "$LINKER_FIXTURE")"
: >"$LINKER_FIXTURE"
chmod +x "$LINKER_FIXTURE"

MINIMAL_ENV_OUTPUT="$TEST_ROOT/minimal-env.out"
if ! env -i \
    HOME="$TEST_ROOT/home" \
    PATH="$JAVA_HOME_FIXTURE/bin:$FAKE_BIN:/usr/bin:/bin" \
    VALIDATE_SLICES_PREVIEW_ENV_ONLY=1 \
    "$VALIDATOR" >"$MINIMAL_ENV_OUTPUT" 2>&1; then
    cat "$MINIMAL_ENV_OUTPUT" >&2
    exit 1
fi
grep -F "resolved_java_home=$JAVA_HOME_FIXTURE" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F "resolved_android_home=$ANDROID_HOME_FIXTURE" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F "resolved_ndk_home=$NDK_HOME_FIXTURE" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F "resolved_cargo=$FAKE_BIN/cargo" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F "resolved_gradle=$FAKE_BIN/gradle" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F "resolved_make=$FAKE_BIN/make" "$MINIMAL_ENV_OUTPUT" >/dev/null
grep -F 'PASS: host validation environment resolved' "$MINIMAL_ENV_OUTPUT" >/dev/null

CONFLICT_ENV_OUTPUT="$TEST_ROOT/conflict-env.out"
env -i \
    HOME="$TEST_ROOT/home" \
    PATH="$JAVA_HOME_FIXTURE/bin:$FAKE_BIN:/usr/bin:/bin" \
    ANDROID_NDK_HOME="$NDK_HOME_FIXTURE" \
    NDK_HOME="$CONFLICT_NDK_FIXTURE" \
    VALIDATE_SLICES_PREVIEW_ENV_ONLY=1 \
    "$VALIDATOR" >"$CONFLICT_ENV_OUTPUT" 2>&1
grep -F "resolved_ndk_home=$NDK_HOME_FIXTURE" "$CONFLICT_ENV_OUTPUT" >/dev/null

BUILD_ENV_OUTPUT="$TEST_ROOT/build-env.out"
EXTERNAL_TARGET_DIR="$TEST_ROOT/external-target"
if ! env -i \
    HOME="$TEST_ROOT/home" \
    PATH="$FAKE_BIN:/usr/bin:/bin" \
    CARGO="$FAKE_BIN/cargo" \
    RUST_TOOLCHAIN_BIN="$FAKE_BIN" \
    ANDROID_NDK_HOME="$NDK_HOME_FIXTURE" \
    NDK_HOME="$CONFLICT_NDK_FIXTURE" \
    CARGO_TARGET_DIR="$EXTERNAL_TARGET_DIR" \
    BUILD_SLOT_RUNTIME_ANDROID_ENV_ONLY=1 \
    "$BUILD_SCRIPT" >"$BUILD_ENV_OUTPUT" 2>&1; then
    cat "$BUILD_ENV_OUTPUT" >&2
    exit 1
fi
grep -F "resolved_cargo=$FAKE_BIN/cargo" "$BUILD_ENV_OUTPUT" >/dev/null
grep -F "resolved_rust_toolchain_bin=$FAKE_BIN" "$BUILD_ENV_OUTPUT" >/dev/null
grep -F "resolved_ndk_home=$NDK_HOME_FIXTURE" "$BUILD_ENV_OUTPUT" >/dev/null
grep -F "resolved_linker=$LINKER_FIXTURE" "$BUILD_ENV_OUTPUT" >/dev/null
grep -F "resolved_target_dir=$EXTERNAL_TARGET_DIR" "$BUILD_ENV_OUTPUT" >/dev/null

OVERRIDE_LINKER="$TEST_ROOT/override-linker"
: >"$OVERRIDE_LINKER"
chmod +x "$OVERRIDE_LINKER"
OVERRIDE_OUTPUT="$TEST_ROOT/build-override.out"
env -i \
    HOME="$TEST_ROOT/home" \
    PATH="$FAKE_BIN:/usr/bin:/bin" \
    CARGO="$FAKE_BIN/cargo" \
    RUST_TOOLCHAIN_BIN="$FAKE_BIN" \
    ANDROID_NDK_HOME="$NDK_HOME_FIXTURE" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$OVERRIDE_LINKER" \
    BUILD_SLOT_RUNTIME_ANDROID_ENV_ONLY=1 \
    "$BUILD_SCRIPT" >"$OVERRIDE_OUTPUT" 2>&1
grep -F "resolved_linker=$OVERRIDE_LINKER" "$OVERRIDE_OUTPUT" >/dev/null

rm -f -- "$FAKE_BIN/cargo"
MISSING_TOOL_OUTPUT="$TEST_ROOT/missing-cargo.out"
if env -i \
    HOME="$TEST_ROOT/home" \
    PATH="$JAVA_HOME_FIXTURE/bin:$FAKE_BIN:/usr/bin:/bin" \
    VALIDATE_SLICES_PREVIEW_ENV_ONLY=1 \
    "$VALIDATOR" >"$MISSING_TOOL_OUTPUT" 2>&1; then
    printf '%s\n' 'expected missing cargo validation to fail' >&2
    exit 1
fi
grep -F 'FAIL: missing host tool: cargo (set CARGO or add cargo to PATH)' \
    "$MISSING_TOOL_OUTPUT" >/dev/null

for script in "$SCRIPT_UNDER_TEST" "$BUILD_SCRIPT"; do
    if grep -Fq '/Users/jianchen' "$script"; then
        printf 'personal absolute path in %s\n' "$script" >&2
        exit 1
    fi
done

printf '%s\n' 'PASS: validate-slices-preview environment resolution fixture'
