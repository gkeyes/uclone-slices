#!/system/bin/sh

set -u

PACKAGE=${1:-com.uclone.slices.fixture}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
AGGREGATE=$RUNTIME_ROOT/packages/$PACKAGE/aggregate.json
MOUNTINFO=/proc/1/mountinfo
SLOTCTL=/data/adb/modules/uclone-slices-v2/bin/slotctl
CE_MARKER=/data/user/0/$PACKAGE/files/identity.txt
DE_MARKER=/data/user_de/0/$PACKAGE/files/identity.txt

read_marker() {
    marker=$1
    if [ -f "$marker" ]; then
        sed -n '1p' "$marker"
    else
        printf '%s' '<absent>'
    fi
}

mounted_slot() {
    target=$1
    grep " $target " "$MOUNTINFO" 2>/dev/null |
        grep -F "/uclone-slices-v2/slots/$PACKAGE/" |
        tail -n 1 |
        sed -n "s#.*uclone-slices-v2/slots/$PACKAGE/\\([^ /]*\\) $target .*#\\1#p"
}

printf '%s\n' '=== UClone Fixture reboot probe ==='
printf '%s\n' 'script_version=1'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'package=%s\n' "$PACKAGE"
printf 'uptime=%s\n' "$(sed -n '1p' /proc/uptime 2>/dev/null)"
printf 'user0_state=%s\n' "$(cmd activity get-started-user-state 0 2>&1)"
printf 'boot_completed=%s\n' "$(getprop sys.boot_completed 2>/dev/null)"
printf 'manager_pid=%s\n' "$(pidof com.uclone.slices.v2 2>/dev/null)"
printf 'fixture_pid=%s\n' "$(pidof "$PACKAGE" 2>/dev/null)"

RPC_OUTPUT=
if [ -x "$SLOTCTL" ]; then
    RPC_OUTPUT=$(
        printf '{"op":"probe"}\n' |
            /system/bin/toybox timeout -s 9 5 "$SLOTCTL" rpc 2>&1
    )
fi
printf 'runtime_probe=%s\n' "$RPC_OUTPUT"

if [ ! -f "$AGGREGATE" ]; then
    printf '%s\n' 'aggregate=missing'
    printf 'ce_marker=%s\n' "$(read_marker "$CE_MARKER")"
    printf 'de_marker=%s\n' "$(read_marker "$DE_MARKER")"
    printf '%s\n' 'RESULT=not_enrolled'
    exit 0
fi

AGGREGATE_JSON=$(sed -n '1p' "$AGGREGATE")
ACTIVE_SLOT=$(
    printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"active_slot":"\([^"]*\)".*/\1/p'
)
LIFECYCLE=$(
    printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"lifecycle":{"state":"\([^"]*\)".*/\1/p'
)
CE_SLOT=$(mounted_slot "/data/user/0/$PACKAGE")
DE_SLOT=$(mounted_slot "/data/user_de/0/$PACKAGE")
CE_VIEW=${CE_SLOT:-base}
DE_VIEW=${DE_SLOT:-base}

if [ "$CE_VIEW" = "$DE_VIEW" ]; then
    OBSERVED_VIEW=$CE_VIEW
else
    OBSERVED_VIEW=inconsistent
fi

printf 'active_slot=%s\n' "$ACTIVE_SLOT"
printf 'lifecycle=%s\n' "$LIFECYCLE"
printf 'ce_view=%s\n' "$CE_VIEW"
printf 'de_view=%s\n' "$DE_VIEW"
printf 'observed_view=%s\n' "$OBSERVED_VIEW"
printf 'ce_marker=%s\n' "$(read_marker "$CE_MARKER")"
printf 'de_marker=%s\n' "$(read_marker "$DE_MARKER")"

if [ "$LIFECYCLE" != ready ]; then
    RESULT=pending_lifecycle
elif [ "$OBSERVED_VIEW" = inconsistent ]; then
    RESULT=inconsistent_view
elif [ "$ACTIVE_SLOT" = "$OBSERVED_VIEW" ]; then
    RESULT=matched_persisted_view
elif [ "$ACTIVE_SLOT" != base ] && [ "$OBSERVED_VIEW" = base ]; then
    RESULT=persisted_slot_but_base
else
    RESULT=view_mismatch
fi

printf 'RESULT=%s\n' "$RESULT"
