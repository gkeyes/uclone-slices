#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

RUNTIME_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/runtime/Cargo.toml" | head -n 1)
MANAGER_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
HOOK_VERSION=$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$ROOT/launcher-hook/build.gradle.kts")
MODULE_VERSION=$(sed -n 's/^version=//p' "$ROOT/kernelsu/module.prop")
MANAGER_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
FIXTURE_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/device-fixture-app/build.gradle.kts" | head -n 1)
HOOK_VERSION_CODE=$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\)/\1/p' "$ROOT/launcher-hook/build.gradle.kts")
MODULE_VERSION_CODE=$(sed -n 's/^versionCode=//p' "$ROOT/kernelsu/module.prop")
RUST_TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml")
RUST_VERSION=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' "$ROOT/runtime/Cargo.toml")
MANAGER_MIN_SDK=$(sed -n 's/^[[:space:]]*minSdk = \([0-9][0-9]*\)/\1/p' "$ROOT/manager-app/build.gradle.kts")
RUNTIME_ANDROID_API=$(sed -n 's/^API=\([0-9][0-9]*\)/\1/p' "$ROOT/tools/build-kernelsu.sh")

[ "$RUNTIME_VERSION" = "$MANAGER_VERSION" ]
[ "$RUNTIME_VERSION" = "$FIXTURE_VERSION" ]
[ "$RUNTIME_VERSION" = "$HOOK_VERSION" ]
[ "$RUNTIME_VERSION" = "$MODULE_VERSION" ]
[ "$MANAGER_VERSION_CODE" = "$FIXTURE_VERSION_CODE" ]
[ "$MANAGER_VERSION_CODE" = "$HOOK_VERSION_CODE" ]
[ "$MANAGER_VERSION_CODE" = "$MODULE_VERSION_CODE" ]
[ "$RUST_TOOLCHAIN" = "$RUST_VERSION.0" ]
[ "$MANAGER_MIN_SDK" = "$RUNTIME_ANDROID_API" ]
grep -F -x 'com.miui.home' "$ROOT/launcher-hook/src/main/resources/META-INF/xposed/scope.list" >/dev/null
grep -F 'staticScope=true' "$ROOT/launcher-hook/src/main/resources/META-INF/xposed/module.prop" >/dev/null
grep -F 'applicationId = "com.uclone.slices.v2.launcher"' "$ROOT/launcher-hook/build.gradle.kts" >/dev/null
grep -F 'android:protectionLevel="signature"' "$ROOT/manager-app/src/main/AndroidManifest.xml" >/dev/null
[ "$(grep -F -c 'android:permission="com.uclone.slices.v2.permission.DESKTOP_SWITCH"' "$ROOT/manager-app/src/main/AndroidManifest.xml")" = 2 ]
grep -F 'android:foregroundServiceType="shortService"' "$ROOT/manager-app/src/main/AndroidManifest.xml" >/dev/null
grep -F 'const val MARKER_SHORTCUT_ID = "uclone_slices_desktop_switch"' "$ROOT/launcher-hook/src/main/java/com/uclone/slices/v2/launcher/relay/LauncherRelayContract.kt" >/dev/null
