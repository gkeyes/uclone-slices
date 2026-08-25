#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SERVICE="$ROOT/kernelsu/service.sh"
CUSTOMIZE="$ROOT/kernelsu/customize.sh"

bash -n "$SERVICE"
bash -n "$CUSTOMIZE"
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
grep -F 'printf '\''{"op":"probe"}\n'\''' "$SERVICE" >/dev/null
grep -F '"$MODDIR/bin/slotctl" rpc' "$SERVICE" >/dev/null
grep -F 'EXPECTED_PROBE_RESPONSE=$(printf '\''{"ok":{"build_id":"%s"}}'\'' "$BUILD_ID")' "$SERVICE" >/dev/null
grep -F '[ "$PROBE_RESPONSE" = "$EXPECTED_PROBE_RESPONSE" ]' "$SERVICE" >/dev/null
grep -F 'chmod 0600 "$SOCKET"' "$SERVICE" >/dev/null
grep -F 'kill -9 "$PID"' "$SERVICE" >/dev/null
grep -F '/system/bin/nsenter -t 1 -m --' "$SERVICE" >/dev/null
grep -F '"$MODDIR/bin/ucloned"' "$SERVICE" >/dev/null
if grep -E 'package|mount|disable|enable|reconcile|rescue' "$SERVICE" >/dev/null; then
    exit 1
fi
grep -F -x 'id=uclone-slices-v2' "$ROOT/kernelsu/module.prop" >/dev/null
grep -E -x 'runtimeBuildId=[0-9]+\.[0-9]+\.[0-9]+' "$ROOT/kernelsu/module.prop" >/dev/null
