#!/system/bin/sh

set -u

PACKAGE=${1:-com.uclone.slices.fixture}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
AGGREGATE=$RUNTIME_ROOT/packages/$PACKAGE/aggregate.json
SLOTCTL=/data/adb/modules/uclone-slices-v2/bin/slotctl
TOYBOX=/system/bin/toybox
CE_SLOT_ROOT=/data/misc_ce/0/uclone-slices-v2/slots/$PACKAGE
DE_SLOT_ROOT=/data/misc_de/0/uclone-slices-v2/slots/$PACKAGE
BASE_CE=/data/user/0/$PACKAGE
BASE_DE=/data/user_de/0/$PACKAGE

print_directory() {
    label=$1
    path=$2
    if [ -d "$path" ]; then
        printf '%s=present\n' "$label"
        ls -la "$path" 2>&1
    else
        printf '%s=missing\n' "$label"
    fi
}

printf '%s\n' '=== UClone Slices V2 registration probe ==='
printf '%s\n' 'script_version=2'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'package=%s\n' "$PACKAGE"
printf 'date=%s\n' "$(date 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    exit 2
fi

case "$PACKAGE" in
    ''|*[!A-Za-z0-9._]*)
        printf '%s\n' 'RESULT=invalid_package'
        exit 2
        ;;
esac

printf '%s\n' '--- persisted registration ---'
if [ -f "$AGGREGATE" ]; then
    AGGREGATE_JSON=$(cat "$AGGREGATE" 2>&1)
    printf '%s\n' 'aggregate=present'
    printf 'aggregate_json=%s\n' "$AGGREGATE_JSON"
    STORED_UID=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"uid":\([0-9][0-9]*\).*/\1/p')
    STORED_APK_PATH=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"apk_path":"\([^"]*\)".*/\1/p')
    STORED_APK_DEVICE=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"apk_device":\([0-9][0-9]*\).*/\1/p')
    STORED_APK_INODE=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"apk_inode":\([0-9][0-9]*\).*/\1/p')
    STORED_ACTIVE=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"active_slot":"\([^"]*\)".*/\1/p')
    STORED_LIFECYCLE=$(printf '%s\n' "$AGGREGATE_JSON" |
        sed -n 's/.*"lifecycle":{"state":"\([^"]*\)".*/\1/p')
else
    AGGREGATE_JSON=
    STORED_UID=
    STORED_APK_PATH=
    STORED_APK_DEVICE=
    STORED_APK_INODE=
    STORED_ACTIVE=
    STORED_LIFECYCLE=
    printf '%s\n' 'aggregate=missing'
fi
printf 'stored_uid=%s\n' "$STORED_UID"
printf 'stored_apk_path=%s\n' "$STORED_APK_PATH"
printf 'stored_apk_device=%s\n' "$STORED_APK_DEVICE"
printf 'stored_apk_inode=%s\n' "$STORED_APK_INODE"
printf 'stored_active_slot=%s\n' "$STORED_ACTIVE"
printf 'stored_lifecycle=%s\n' "$STORED_LIFECYCLE"

printf '%s\n' '--- current package identity ---'
THIRD_PARTY_OUTPUT=$(cmd package list packages -3 -U "$PACKAGE" 2>&1)
THIRD_PARTY_STATUS=$?
THIRD_PARTY_LINE=$(printf '%s\n' "$THIRD_PARTY_OUTPUT" |
    sed -n "/^package:$PACKAGE uid:[0-9][0-9]*$/p" |
    sed -n '1p')
CURRENT_UID=$(printf '%s\n' "$THIRD_PARTY_LINE" |
    sed -n 's/.* uid:\([0-9][0-9]*\)$/\1/p')
PATH_OUTPUT=$(pm path "$PACKAGE" 2>&1)
PATH_STATUS=$?
CURRENT_APK_PATH=$(printf '%s\n' "$PATH_OUTPUT" |
    sed -n 's/^package://p' |
    sed -n '1p')
STAT_OUTPUT=
STAT_STATUS=1
CURRENT_APK_DEVICE=
CURRENT_APK_INODE=
if [ -n "$CURRENT_APK_PATH" ]; then
    STAT_OUTPUT=$("$TOYBOX" stat -c '%d %i' "$CURRENT_APK_PATH" 2>&1)
    STAT_STATUS=$?
    if [ "$STAT_STATUS" -eq 0 ]; then
        CURRENT_APK_DEVICE=${STAT_OUTPUT%% *}
        CURRENT_APK_INODE=${STAT_OUTPUT#* }
    fi
fi
printf 'third_party_exit=%s\n' "$THIRD_PARTY_STATUS"
printf 'third_party_line=%s\n' "$THIRD_PARTY_LINE"
printf 'current_uid=%s\n' "$CURRENT_UID"
printf 'package_path_exit=%s\n' "$PATH_STATUS"
printf 'current_apk_path=%s\n' "$CURRENT_APK_PATH"
printf 'apk_stat_exit=%s\n' "$STAT_STATUS"
printf 'current_apk_device=%s\n' "$CURRENT_APK_DEVICE"
printf 'current_apk_inode=%s\n' "$CURRENT_APK_INODE"

printf '%s\n' '--- Base and slot storage ---'
print_directory base_ce "$BASE_CE"
print_directory base_de "$BASE_DE"
print_directory ce_slot_root "$CE_SLOT_ROOT"
print_directory de_slot_root "$DE_SLOT_ROOT"

printf '%s\n' '--- get_package result ---'
RPC_INVOKED=no
RPC_STATUS=not_run
RPC_OUTPUT=not_run_identity_matches
IDENTITY_MATCHES=no
if [ -n "$STORED_UID" ] &&
    [ "$STORED_UID" = "$CURRENT_UID" ] &&
    [ "$STORED_APK_PATH" = "$CURRENT_APK_PATH" ] &&
    [ "$STORED_APK_DEVICE" = "$CURRENT_APK_DEVICE" ] &&
    [ "$STORED_APK_INODE" = "$CURRENT_APK_INODE" ]; then
    IDENTITY_MATCHES=yes
else
    RPC_INVOKED=yes
    RPC_OUTPUT=$(
        printf '{"op":"get_package","package":"%s"}\n' "$PACKAGE" |
            "$TOYBOX" timeout -s 9 5 "$SLOTCTL" rpc 2>&1
    )
    RPC_STATUS=$?
fi
printf 'identity_matches=%s\n' "$IDENTITY_MATCHES"
printf 'rpc_invoked=%s\n' "$RPC_INVOKED"
printf 'rpc_exit=%s\n' "$RPC_STATUS"
printf 'rpc_output=%s\n' "$RPC_OUTPUT"

printf '%s\n' '--- classification ---'
if [ ! -f "$AGGREGATE" ]; then
    RESULT=not_registered
elif [ -z "$THIRD_PARTY_LINE" ] || [ -z "$CURRENT_APK_PATH" ] || [ "$STAT_STATUS" -ne 0 ]; then
    RESULT=current_package_unavailable
elif [ "$IDENTITY_MATCHES" = yes ]; then
    RESULT=registration_identity_matches
else
    RESULT=registration_probe_inconclusive
    if [ "$RPC_STATUS" -eq 0 ]; then
        case "$RPC_OUTPUT" in
            *'"state_conflict"'*)
                if [ -n "$STORED_UID" ] &&
                    [ -n "$STORED_APK_PATH" ] &&
                    [ -n "$STORED_APK_DEVICE" ] &&
                    [ -n "$STORED_APK_INODE" ]; then
                    RESULT=package_identity_changed
                else
                    RESULT=aggregate_invalid
                fi
                ;;
        esac
    fi
fi
printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' '=== end ==='
