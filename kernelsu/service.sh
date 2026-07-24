#!/system/bin/sh

MODDIR=${0%/*}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
SOCKET=$RUNTIME_ROOT/runtime.sock
PID_FILE=$RUNTIME_ROOT/ucloned.pid
BUILD_ID=$(sed -n 's/^version=//p' "$MODDIR/module.prop")

mkdir -p "$RUNTIME_ROOT" || exit 1
[ -n "$BUILD_ID" ] || exit 1

if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE" 2>/dev/null)
    if [ -n "$PID" ] &&
        kill -0 "$PID" 2>/dev/null &&
        [ "$(readlink "/proc/$PID/exe" 2>/dev/null)" = "$MODDIR/bin/ucloned" ]; then
        exit 0
    fi
fi

rm -f "$SOCKET"
UCLONE_RUNTIME_ROOT="$RUNTIME_ROOT" \
UCLONE_RUNTIME_SOCKET="$SOCKET" \
UCLONE_BUILD_ID="$BUILD_ID" \
/system/bin/nsenter -t 1 -m -- \
"$MODDIR/bin/ucloned" >>"$RUNTIME_ROOT/ucloned.log" 2>&1 &
echo "$!" >"$PID_FILE"
