#!/usr/bin/env bash
set -euo pipefail

ADB=${ADB:-/Users/jianchen/Downloads/adb/platform-tools/adb}
SERIAL_ARGS=()
if [ -n "${ANDROID_SERIAL:-}" ]; then
    SERIAL_ARGS=(-s "$ANDROID_SERIAL")
fi

"$ADB" "${SERIAL_ARGS[@]}" get-state >/dev/null
LAUNCHER_DUMP=$("$ADB" "${SERIAL_ARGS[@]}" shell dumpsys package com.miui.home)
printf '%s\n' "$LAUNCHER_DUMP" | grep -F 'versionCode=801025341' >/dev/null
printf '%s\n' "$LAUNCHER_DUMP" | grep -F 'versionName=RELEASE-8.01.02.5341-260807-08151903-R' >/dev/null

LOGS=$("$ADB" "${SERIAL_ARGS[@]}" logcat -d -s UCloneSlicesLauncher:I '*:S')
printf '%s\n' "$LOGS" | grep -F 'hooked LauncherApps.getShortcuts' >/dev/null
printf '%s\n' "$LOGS" | grep -F 'hooked LauncherApps.startShortcut overloads' >/dev/null
printf '%s\n' "$LOGS" | grep -F 'getShortcuts inject package=com.uclone.slices.fixture user=0 title=Slices Hook 探针' >/dev/null
printf '%s\n' "$LOGS" | grep -F 'startShortcut probe passed package=com.uclone.slices.fixture user=0' >/dev/null

printf '%s\n' 'LauncherApps Hook probe passed without a Slices data operation.'
