#!/usr/bin/env bash

set -euo pipefail
umask 077

if (( $# == 0 )); then
    PROFILE=slotprobe
elif (( $# == 2 )) && [ "$1" = --profile ]; then
    PROFILE=$2
else
    printf '%s\n' 'usage: tools/build-slot-runtime-android.sh [--profile slotprobe|fitness|generic]' >&2
    exit 2
fi
case "$PROFILE" in slotprobe|fitness|generic) ;; *) exit 2 ;; esac

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)"
RUNTIME_ROOT="$REPO_ROOT/slot-runtime"
BASE_PATH="${PATH:-/usr/bin:/bin:/usr/sbin:/sbin}"
TOOLCHAIN_BIN="${RUST_TOOLCHAIN_BIN:-}"
if [ -n "$TOOLCHAIN_BIN" ]; then
    [ -d "$TOOLCHAIN_BIN" ] || { printf 'missing Rust toolchain directory: %s\n' "$TOOLCHAIN_BIN" >&2; exit 1; }
    export PATH="$TOOLCHAIN_BIN:$BASE_PATH"
fi
CARGO="${CARGO:-}"
if [ -n "$CARGO" ]; then
    case "$CARGO" in */*) ;; *) CARGO=$(command -v "$CARGO" 2>/dev/null || true);; esac
else
    CARGO=$(command -v cargo 2>/dev/null || true)
fi
[ -n "$CARGO" ] && [ -x "$CARGO" ] || { printf '%s\n' 'missing host tool: cargo (set CARGO or add cargo to PATH)' >&2; exit 1; }
if [ -z "$TOOLCHAIN_BIN" ]; then
    TOOLCHAIN_BIN=$(CDPATH= cd -- "$(dirname -- "$CARGO")" && pwd -P)
    export PATH="$TOOLCHAIN_BIN:$BASE_PATH"
fi
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$ANDROID_HOME" ]; then
    for sdk_candidate in "${HOME:-}/Library/Android/sdk" "${HOME:-}/Android/Sdk" /usr/local/share/android-commandlinetools /opt/homebrew/share/android-commandlinetools; do
        [ -n "$ANDROID_HOME" ] || [ ! -d "$sdk_candidate" ] || ANDROID_HOME="$sdk_candidate"
    done
fi
NDK_ROOT="${ANDROID_NDK_HOME:-${NDK_HOME:-}}"
if [ -z "$NDK_ROOT" ]; then
    NDK_ROOT="$ANDROID_HOME/ndk/29.0.14206865"
    for ndk_candidate in "$ANDROID_HOME"/ndk/29.*; do
        [ -d "$NDK_ROOT" ] || [ ! -d "$ndk_candidate" ] || NDK_ROOT="$ndk_candidate"
    done
fi
[ -d "$NDK_ROOT" ] || { printf 'missing host tool: Android NDK 29 (set NDK_HOME or ANDROID_NDK_HOME)\n' >&2; exit 1; }
TARGET="aarch64-linux-android"
case "${CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER:-}" in
    '')
        case "$(uname -s):$(uname -m)" in
            Darwin:arm64) PREBUILT_HOST=darwin-arm64 ;;
            Darwin:x86_64) PREBUILT_HOST=darwin-x86_64 ;;
            Linux:x86_64) PREBUILT_HOST=linux-x86_64 ;;
            *) printf 'unsupported Android NDK host: %s/%s\n' "$(uname -s)" "$(uname -m)" >&2; exit 1 ;;
        esac
        LINKER="$NDK_ROOT/toolchains/llvm/prebuilt/$PREBUILT_HOST/bin/aarch64-linux-android29-clang"
        ;;
    *) LINKER="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER" ;;
esac
EVIDENCE_DIR="$REPO_ROOT/.omo/evidence/slices-preview-validation"
LOG_PATH="$EVIDENCE_DIR/rust-android-aarch64-$PROFILE.log"
PROFILE_TARGET_DIR="$RUNTIME_ROOT/target-profiles/$PROFILE"
if [ -n "${CARGO_TARGET_DIR:-}" ]; then PROFILE_TARGET_DIR="$CARGO_TARGET_DIR"; fi
PROFILE_OUTPUT="$PROFILE_TARGET_DIR/profile"
export ANDROID_NDK_HOME="$NDK_ROOT" NDK_HOME="$NDK_ROOT"

require_file() {
    path="$1"
    [ -f "$path" ] || { printf 'missing file: %s\n' "$path" >&2; exit 1; }
    [ ! -L "$path" ] || { printf 'symlink rejected: %s\n' "$path" >&2; exit 1; }
}

require_executable() {
    require_file "$1"
    [ -x "$1" ] || { printf 'not executable: %s\n' "$1" >&2; exit 1; }
}

require_tool() {
    [ -x "$1" ] || { printf 'missing host tool: %s\n' "$1" >&2; exit 1; }
}

require_tool "$CARGO"
require_tool "$LINKER"
require_file "$NDK_ROOT/source.properties"
grep -Eq '^Pkg\.Revision = 29\.' "$NDK_ROOT/source.properties" || {
    printf '%s\n' 'Android NDK 29 is required' >&2
    exit 1
}
require_file "$RUNTIME_ROOT/.cargo/config.toml"
if [ "${BUILD_SLOT_RUNTIME_ANDROID_ENV_ONLY:-0}" = 1 ]; then
    printf 'resolved_cargo=%s\nresolved_rust_toolchain_bin=%s\nresolved_ndk_home=%s\nresolved_linker=%s\nresolved_target_dir=%s\n' \
        "$CARGO" "$TOOLCHAIN_BIN" "$NDK_ROOT" "$LINKER" "$PROFILE_TARGET_DIR"
    exit 0
fi

mkdir -p "$EVIDENCE_DIR"
mkdir -p "$PROFILE_OUTPUT"
GENERATED_PROFILE="$PROFILE_OUTPUT/target-profile-$PROFILE"
if [ -d "$GENERATED_PROFILE" ]; then
    chmod -R u+w "$GENERATED_PROFILE"
    rm -rf -- "$GENERATED_PROFILE"
fi
"$REPO_ROOT/tools/render-target-profile.sh" "$PROFILE" "$PROFILE_OUTPUT" >/dev/null
(
    cd "$RUNTIME_ROOT"
    export CARGO_TARGET_DIR="$PROFILE_TARGET_DIR"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER"
    export CARGO_BUILD_JOBS=1
    export UCLONE_TARGET_PROFILE="$PROFILE"
    unset CARGO_BUILD_TARGET RUSTFLAGS RUSTC_WRAPPER
    printf 'repository=%s\n' "$REPO_ROOT"
    printf 'rust_toolchain=1.95.0\n'
    printf 'target=%s\n' "$TARGET"
    printf 'profile=%s\n' "$PROFILE"
    printf 'ndk=%s\n' "$NDK_ROOT"
    printf 'command='; printf '%q ' "$CARGO" build --locked --offline --release --target "$TARGET" --bins
    printf '\n'
    CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 \
        "$CARGO" build --locked --offline --release --target "$TARGET" --bins
) >"$LOG_PATH" 2>&1

for binary in ucloned slotctl; do
    artifact="$PROFILE_TARGET_DIR/$TARGET/release/$binary"
    require_executable "$artifact"
done

printf '%s\n' "PASS: Android $TARGET Rust binaries built with NDK 29 (see $LOG_PATH)"
