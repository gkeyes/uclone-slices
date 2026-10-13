#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

RUNTIME_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/runtime/Cargo.toml" | head -n 1)
MANAGER_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
MODULE_VERSION=$(sed -n 's/^version=//p' "$ROOT/kernelsu/module.prop")
MANAGER_PROTOCOL_BUILD_ID=$(sed -n 's/.*RUNTIME_PROTOCOL_BUILD_ID", "\\"\([^\"]*\)\\"".*/\1/p' "$ROOT/manager-app/build.gradle.kts")
MODULE_RUNTIME_BUILD_ID=$(sed -n 's/^runtimeBuildId=//p' "$ROOT/kernelsu/module.prop")
MANAGER_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
MODULE_VERSION_CODE=$(sed -n 's/^versionCode=//p' "$ROOT/kernelsu/module.prop")
MANAGER_MIN_SDK=$(sed -n 's/^[[:space:]]*minSdk = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
RUNTIME_ANDROID_API=$(sed -n 's/^API=\([0-9][0-9]*\)/\1/p' "$ROOT/tools/build-kernelsu.sh")
HELPER_ANDROID_API=$(sed -n 's/^API=\([0-9][0-9]*\)/\1/p' "$ROOT/tools/build-manager-helper.sh")

FAILED=0
require_equal() {
    if [ "$2" != "$3" ]; then
        printf '%s: %s != %s\n' "$1" "$2" "$3" >&2
        FAILED=1
    fi
}

require_equal "Manager/fixture versionName" "$MANAGER_VERSION" "$FIXTURE_VERSION"
require_equal "Runtime crate/module version" "$RUNTIME_VERSION" "$MODULE_VERSION"
require_equal "Manager/module version" "$MANAGER_VERSION" "$MODULE_VERSION"
require_equal "Manager/module protocol id" "$MANAGER_PROTOCOL_BUILD_ID" "$MODULE_RUNTIME_BUILD_ID"
require_equal "Manager/fixture versionCode" "$MANAGER_VERSION_CODE" "$FIXTURE_VERSION_CODE"
require_equal "Manager/module versionCode" "$MANAGER_VERSION_CODE" "$MODULE_VERSION_CODE"
require_equal "Manager minSdk/runtime API" "$MANAGER_MIN_SDK" "$RUNTIME_ANDROID_API"
require_equal "Manager minSdk/helper API" "$MANAGER_MIN_SDK" "$HELPER_ANDROID_API"
exit "$FAILED"
