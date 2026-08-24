#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
NDK_ROOT=${ANDROID_NDK_HOME:?ANDROID_NDK_HOME is required}
TARGET=aarch64-linux-android
API=29

case "$(uname -s)-$(uname -m)" in
    Darwin-x86_64) HOST_TAG=darwin-x86_64 ;;
    Darwin-arm64) HOST_TAG=darwin-arm64 ;;
    Linux-x86_64) HOST_TAG=linux-x86_64 ;;
    Linux-aarch64) HOST_TAG=linux-aarch64 ;;
    *) exit 1 ;;
esac

LINKER="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/${TARGET}${API}-clang"
CXX="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/${TARGET}${API}-clang++"
AR="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/llvm-ar"
STRIP="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/llvm-strip"
[ -x "$LINKER" ]
[ -x "$CXX" ]
[ -x "$AR" ]
[ -x "$STRIP" ]

TOOLCHAIN=$(rustup show active-toolchain | awk '{print $1}')
rustup target add "$TARGET" --toolchain "$TOOLCHAIN"
CC_aarch64_linux_android="$LINKER" \
    CXX_aarch64_linux_android="$CXX" \
    AR_aarch64_linux_android="$AR" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER" \
    rustup run "$TOOLCHAIN" cargo build \
    --manifest-path "$ROOT/runtime/Cargo.toml" \
    --release \
    --locked \
    --target "$TARGET" \
    --bin uclone_archive

mkdir -p "$ROOT/outputs/helpers"
install -m 0755 \
    "$ROOT/target/$TARGET/release/uclone_archive" \
    "$ROOT/outputs/helpers/uclone_archive"
"$STRIP" --strip-all "$ROOT/outputs/helpers/uclone_archive"
sha256sum "$ROOT/outputs/helpers/uclone_archive" | awk '{print $1}' \
    > "$ROOT/outputs/helpers/uclone_archive.sha256"
