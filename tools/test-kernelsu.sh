#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SERVICE="$ROOT/kernelsu/service.sh"
CUSTOMIZE="$ROOT/kernelsu/customize.sh"

bash -n "$SERVICE"
bash -n "$CUSTOMIZE"
grep -F -x 'RUNTIME_ROOT=/data/adb/uclone-slices-v2' "$SERVICE" >/dev/null
grep -F 'readlink "/proc/$PID/exe"' "$SERVICE" >/dev/null
grep -F '[ -S "$SOCKET" ]' "$SERVICE" >/dev/null
grep -F 'printf '\''{"op":"probe"}\n'\''' "$SERVICE" >/dev/null
grep -F '"$MODDIR/bin/slotctl" rpc' "$SERVICE" >/dev/null
grep -F ''\''{"ok":'\''*|'\''{"error":'\''*)' "$SERVICE" >/dev/null
grep -F 'kill -9 "$PID"' "$SERVICE" >/dev/null
grep -F '/system/bin/nsenter -t 1 -m --' "$SERVICE" >/dev/null
grep -F '"$MODDIR/bin/ucloned"' "$SERVICE" >/dev/null
if grep -E 'package|mount|disable|enable|reconcile|rescue' "$SERVICE" >/dev/null; then
    exit 1
fi
grep -F -x 'id=uclone-slices-v2' "$ROOT/kernelsu/module.prop" >/dev/null
