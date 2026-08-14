#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

RUNTIME_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/runtime/Cargo.toml" | head -n 1)
MANAGER_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
MODULE_VERSION=$(sed -n 's/^version=//p' "$ROOT/kernelsu/module.prop")
MANAGER_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
MODULE_VERSION_CODE=$(sed -n 's/^versionCode=//p' "$ROOT/kernelsu/module.prop")
RUST_TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml")
RUST_VERSION=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' "$ROOT/runtime/Cargo.toml")
MANAGER_MIN_SDK=$(sed -n 's/^[[:space:]]*minSdk = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
RUNTIME_ANDROID_API=$(sed -n 's/^API=\([0-9][0-9]*\)/\1/p' "$ROOT/tools/build-kernelsu.sh")

[ "$RUNTIME_VERSION" = "$MANAGER_VERSION" ]
[ "$RUNTIME_VERSION" = "$FIXTURE_VERSION" ]
[ "$RUNTIME_VERSION" = "$MODULE_VERSION" ]
[ "$MANAGER_VERSION_CODE" = "$FIXTURE_VERSION_CODE" ]
[ "$MANAGER_VERSION_CODE" = "$MODULE_VERSION_CODE" ]
[ "$RUST_TOOLCHAIN" = "$RUST_VERSION.0" ]
[ "$MANAGER_MIN_SDK" = "$RUNTIME_ANDROID_API" ]
