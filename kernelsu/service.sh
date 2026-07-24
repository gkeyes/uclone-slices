#!/system/bin/sh

MODDIR=${0%/*}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
SOCKET=$RUNTIME_ROOT/runtime.sock
PID_FILE=$RUNTIME_ROOT/ucloned.pid
BUILD_ID=$(sed -n 's/^version=//p' "$MODDIR/module.prop")
PID=
PID_EXE=

mkdir -p "$RUNTIME_ROOT" || exit 1
[ -n "$BUILD_ID" ] || exit 1

runtime_transport_ready() {
    [ -f "$PID_FILE" ] || return 1
    PID=$(cat "$PID_FILE" 2>/dev/null)
    case "$PID" in
        ''|*[!0-9]*) return 1 ;;
    esac
    kill -0 "$PID" 2>/dev/null || return 1
    PID_EXE=$(readlink "/proc/$PID/exe" 2>/dev/null)
    [ "$PID_EXE" = "$MODDIR/bin/ucloned" ] || return 1
    [ -S "$SOCKET" ] || return 1
    PROBE_RESPONSE=$(
        printf '{"op":"probe"}\n' |
            "$MODDIR/bin/slotctl" rpc 2>/dev/null
    ) || return 1
    case "$PROBE_RESPONSE" in
        '{"ok":'*|'{"error":'*) return 0 ;;
        *) return 1 ;;
    esac
}

if runtime_transport_ready; then
    exit 0
fi

case "$PID_EXE" in
    "$MODDIR/bin/ucloned"|"$MODDIR/bin/ucloned (deleted)")
        kill -9 "$PID" 2>/dev/null
        ;;
esac

rm -f "$PID_FILE" "$SOCKET"
UCLONE_RUNTIME_ROOT="$RUNTIME_ROOT" \
UCLONE_RUNTIME_SOCKET="$SOCKET" \
UCLONE_BUILD_ID="$BUILD_ID" \
/system/bin/nsenter -t 1 -m -- \
"$MODDIR/bin/ucloned" >>"$RUNTIME_ROOT/ucloned.log" 2>&1 &
echo "$!" >"$PID_FILE"
