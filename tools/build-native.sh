#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
NDK_ROOT=${ANDROID_NDK_HOME:?ANDROID_NDK_HOME is required}
API=29
BUILD_ID=$(sed -n 's/^runtimeBuildId=//p' "$ROOT/kernelsu/module.prop")
case "$(uname -s)-$(uname -m)" in
    Darwin-x86_64) HOST_TAG=darwin-x86_64 ;;
    Darwin-arm64) HOST_TAG=darwin-arm64 ;;
    Linux-x86_64) HOST_TAG=linux-x86_64 ;;
    *) exit 1 ;;
esac
CXX="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/aarch64-linux-android${API}-clang++"
STRIP="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/llvm-strip"
OUT="$ROOT/outputs/native"
mkdir -p "$OUT"
FLAGS=(-std=c++20 -O2 -Wall -Wextra -Werror -fno-exceptions -fno-rtti
       -fvisibility=hidden -fvisibility-inlines-hidden -static-libstdc++
       '-Wl,--exclude-libs,ALL' '-Wl,-z,relro,-z,now'
       "-DUCLONE_BUILD_ID=\"$BUILD_ID\"")
"$CXX" "${FLAGS[@]}" -fPIC -shared "$ROOT/native/slice_zygisk.cpp" -o "$OUT/arm64-v8a.so"
"$CXX" "${FLAGS[@]}" "$ROOT/native/process_view.cpp" -o "$OUT/uclone-process-view"
"$CXX" "${FLAGS[@]}" "$ROOT/native/namespace_anchor.cpp" -o "$OUT/uclone-namespace-anchor"
"$STRIP" --strip-all "$OUT/arm64-v8a.so" "$OUT/uclone-process-view" "$OUT/uclone-namespace-anchor"
