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
STRIP="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/llvm-strip"
[ -x "$LINKER" ]
[ -x "$STRIP" ]

TOOLCHAIN=$(rustup show active-toolchain | awk '{print $1}')
rustup target add "$TARGET" --toolchain "$TOOLCHAIN"
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER" \
    rustup run "$TOOLCHAIN" cargo build \
    --manifest-path "$ROOT/runtime/Cargo.toml" \
    --release \
    --locked \
    --target "$TARGET" \
    --bins

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin" "$ROOT/outputs"
cp "$ROOT/kernelsu/module.prop" "$STAGE/module.prop"
cp "$ROOT/kernelsu/customize.sh" "$STAGE/customize.sh"
cp "$ROOT/kernelsu/service.sh" "$STAGE/service.sh"
cp "$ROOT/target/$TARGET/release/ucloned" "$STAGE/bin/ucloned"
cp "$ROOT/target/$TARGET/release/slotctl" "$STAGE/bin/slotctl"
"$STRIP" --strip-all "$STAGE/bin/ucloned" "$STAGE/bin/slotctl"
chmod 0755 "$STAGE/customize.sh" "$STAGE/service.sh" "$STAGE/bin/ucloned" "$STAGE/bin/slotctl"

(
    cd "$STAGE"
    zip -9 -X -q -r "$ROOT/outputs/uclone-slices-v2-kernelsu.zip" .
)
unzip -t "$ROOT/outputs/uclone-slices-v2-kernelsu.zip" >/dev/null
