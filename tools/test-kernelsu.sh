#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SERVICE="$ROOT/kernelsu/service.sh"
CUSTOMIZE="$ROOT/kernelsu/customize.sh"

bash -n "$SERVICE"
bash -n "$CUSTOMIZE"
bash -n "$ROOT/kernelsu/runtime-entry.sh"
bash -n "$ROOT/kernelsu/post-fs-data.sh"
bash -n "$ROOT/kernelsu/uninstall.sh"
grep -F -x 'umask 077' "$SERVICE" >/dev/null
grep -F -x 'RUNTIME_ROOT=/data/adb/uclone-slices-v2' "$SERVICE" >/dev/null
grep -F 'CE_STORAGE_ROOT=/data/misc_ce/0/uclone-slices-v2' "$SERVICE" >/dev/null
grep -F 'DE_STORAGE_ROOT=/data/misc_de/0/uclone-slices-v2' "$SERVICE" >/dev/null
grep -F 'BUILD_ID=$(sed -n '\''s/^runtimeBuildId=//p'\'' "$MODDIR/module.prop")' "$SERVICE" >/dev/null
grep -F '[ ! -L "$DIRECTORY" ]' "$SERVICE" >/dev/null
grep -F 'chown 0:0 "$DIRECTORY"' "$SERVICE" >/dev/null
grep -F 'chmod 0700 "$DIRECTORY"' "$SERVICE" >/dev/null
grep -F '0:0:700' "$SERVICE" >/dev/null
grep -F 'readlink "/proc/$PID/exe"' "$SERVICE" >/dev/null
grep -F '[ -S "$SOCKET" ]' "$SERVICE" >/dev/null
grep -F 'UCLONE_EXPECTED_BUILD_ID="$BUILD_ID" "$MODDIR/bin/slotctl" check-runtime' "$SERVICE" >/dev/null
grep -F 'chmod 0600 "$SOCKET"' "$SERVICE" >/dev/null
grep -F 'kill -9 "$PID"' "$SERVICE" >/dev/null
grep -F '/system/bin/nsenter -t 1 -m --' "$SERVICE" >/dev/null
grep -F '"$MODDIR/bin/ucloned"' "$SERVICE" >/dev/null
grep -F 'mv -f "$LOG_FILE" "$LOG_FILE.1"' "$SERVICE" >/dev/null
if grep -E 'package|mount|disable|enable|reconcile|rescue' "$SERVICE" >/dev/null; then
    exit 1
fi
grep -F -x 'id=uclone-slices-v2' "$ROOT/kernelsu/module.prop" >/dev/null
grep -E -x 'runtimeBuildId=[0-9]+\.[0-9]+\.[0-9]+' "$ROOT/kernelsu/module.prop" >/dev/null
