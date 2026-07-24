#!/system/bin/sh

set -u
set -f

PACKAGE=${1:-com.xingin.xhs}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
MODULE_ROOT=/data/adb/modules/uclone-slices-v2
AGGREGATE=$RUNTIME_ROOT/packages/$PACKAGE/aggregate.json
PID_FILE=$RUNTIME_ROOT/ucloned.pid
LOG_FILE=$RUNTIME_ROOT/ucloned.log
RUNTIME_BIN=$MODULE_ROOT/bin/ucloned
CANONICAL_CE=/data/user/0/$PACKAGE
CANONICAL_DE=/data/user_de/0/$PACKAGE
TOYBOX=/system/bin/toybox
CMD=/system/bin/cmd

json_string() {
    key=$1
    "$TOYBOX" sed -n "s/.*\"$key\":\"\\([^\"]*\\)\".*/\\1/p" "$AGGREGATE"
}

json_number() {
    key=$1
    "$TOYBOX" sed -n "s/.*\"$key\":\\([0-9][0-9]*\\).*/\\1/p" "$AGGREGATE"
}

slot_from_root() {
    root=$1
    marker="/uclone-slices-v2/slots/$PACKAGE/"
    case "$root" in
        *"$marker"*)
            slot=${root#*"$marker"}
            case "$slot" in
                ''|*/*) ;;
                *) printf '%s\n' "$slot" ;;
            esac
            ;;
    esac
}

printf '%s\n' '=== UClone Slices V2 recovery-popup probe ==='
printf '%s\n' 'script_version=1'
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

printf '%s\n' '--- installed Runtime ---'
MODULE_VERSION=$(
    "$TOYBOX" sed -n 's/^version=//p' "$MODULE_ROOT/module.prop" 2>/dev/null
)
RUNTIME_SHA=$(
    "$TOYBOX" sha256sum "$RUNTIME_BIN" 2>/dev/null |
        "$TOYBOX" sed -n 's/[[:space:]].*//p'
)
printf 'module_version=%s\n' "$MODULE_VERSION"
printf 'runtime_sha256=%s\n' "$RUNTIME_SHA"

printf '%s\n' '--- persisted registration ---'
STORED_UID=
STORED_APK_PATH=
STORED_APK_DEVICE=
STORED_APK_INODE=
ACTIVE_SLOT=
LIFECYCLE=
if [ -f "$AGGREGATE" ]; then
    printf 'aggregate_json='
    "$TOYBOX" cat "$AGGREGATE" 2>&1
    printf '\n'
    STORED_UID=$(json_number uid)
    STORED_APK_PATH=$(json_string apk_path)
    STORED_APK_DEVICE=$(json_number apk_device)
    STORED_APK_INODE=$(json_number apk_inode)
    ACTIVE_SLOT=$(json_string active_slot)
    LIFECYCLE=$(json_string state)
else
    printf '%s\n' 'aggregate=missing'
fi
printf 'stored_uid=%s\n' "$STORED_UID"
printf 'stored_apk_path=%s\n' "$STORED_APK_PATH"
printf 'stored_apk_device=%s\n' "$STORED_APK_DEVICE"
printf 'stored_apk_inode=%s\n' "$STORED_APK_INODE"
printf 'active_slot=%s\n' "$ACTIVE_SLOT"
printf 'lifecycle=%s\n' "$LIFECYCLE"

printf '%s\n' '--- current package identity ---'
CURRENT_UID=
PACKAGE_ROWS=$(
    "$CMD" package list packages -3 -U --user 0 2>&1
)
while IFS= read -r row; do
    case "$row" in
        "package:$PACKAGE uid:"*)
            candidate=${row#"package:$PACKAGE uid:"}
            case "$candidate" in
                ''|*[!0-9]*) ;;
                *) CURRENT_UID=$candidate ;;
            esac
            ;;
    esac
done <<EOF
$PACKAGE_ROWS
EOF

CURRENT_APK_PATH=
PATH_ROWS=$(
    "$CMD" package path --user 0 "$PACKAGE" 2>&1
)
while IFS= read -r row; do
    case "$row" in
        package:*/base.apk)
            CURRENT_APK_PATH=${row#package:}
            break
            ;;
    esac
done <<EOF
$PATH_ROWS
EOF

CURRENT_APK_DEVICE=
CURRENT_APK_INODE=
if [ -n "$CURRENT_APK_PATH" ]; then
    APK_STAT=$(
        "$TOYBOX" stat -c '%d %i' "$CURRENT_APK_PATH" 2>/dev/null
    )
    CURRENT_APK_DEVICE=${APK_STAT%% *}
    CURRENT_APK_INODE=${APK_STAT#* }
fi
printf 'current_uid=%s\n' "$CURRENT_UID"
printf 'current_apk_path=%s\n' "$CURRENT_APK_PATH"
printf 'current_apk_device=%s\n' "$CURRENT_APK_DEVICE"
printf 'current_apk_inode=%s\n' "$CURRENT_APK_INODE"

IDENTITY_MATCHES=no
if [ -n "$STORED_UID" ] &&
    [ "$STORED_UID" = "$CURRENT_UID" ] &&
    [ "$STORED_APK_PATH" = "$CURRENT_APK_PATH" ] &&
    [ "$STORED_APK_DEVICE" = "$CURRENT_APK_DEVICE" ] &&
    [ "$STORED_APK_INODE" = "$CURRENT_APK_INODE" ]; then
    IDENTITY_MATCHES=yes
fi
printf 'identity_matches=%s\n' "$IDENTITY_MATCHES"

printf '%s\n' '--- Runtime mount view ---'
RUNTIME_PID=
RUNTIME_STATE=missing
RUNTIME_MOUNTINFO=
if [ -f "$PID_FILE" ]; then
    RUNTIME_PID=$("$TOYBOX" cat "$PID_FILE" 2>/dev/null)
    case "$RUNTIME_PID" in
        ''|*[!0-9]*) RUNTIME_STATE=invalid_pid ;;
        *)
            if kill -0 "$RUNTIME_PID" 2>/dev/null; then
                RUNTIME_STATE=live
                RUNTIME_MOUNTINFO=/proc/$RUNTIME_PID/mountinfo
            else
                RUNTIME_STATE=stale_pid
            fi
            ;;
    esac
fi
printf 'runtime_pid=%s\n' "$RUNTIME_PID"
printf 'runtime_state=%s\n' "$RUNTIME_STATE"
printf 'runtime_mount_ns=%s\n' \
    "$("$TOYBOX" readlink "/proc/$RUNTIME_PID/ns/mnt" 2>/dev/null)"
printf 'init_mount_ns=%s\n' \
    "$("$TOYBOX" readlink /proc/1/ns/mnt 2>/dev/null)"

CE_COUNT=0
DE_COUNT=0
CE_ROOT=
DE_ROOT=
if [ "$RUNTIME_STATE" = live ] && [ -r "$RUNTIME_MOUNTINFO" ]; then
    while IFS= read -r mount_line; do
        set -- $mount_line
        [ "$#" -ge 5 ] || continue
        mount_root=$4
        mount_point=$5
        case "$mount_point" in
            "$CANONICAL_CE")
                CE_COUNT=$((CE_COUNT + 1))
                CE_ROOT=$mount_root
                ;;
            "$CANONICAL_DE")
                DE_COUNT=$((DE_COUNT + 1))
                DE_ROOT=$mount_root
                ;;
        esac
    done <"$RUNTIME_MOUNTINFO"
fi

OBSERVED_VIEW=unknown
CE_SLOT=$(slot_from_root "$CE_ROOT")
DE_SLOT=$(slot_from_root "$DE_ROOT")
if [ "$RUNTIME_STATE" = live ] && [ -r "$RUNTIME_MOUNTINFO" ]; then
    if [ "$CE_COUNT" -eq 0 ] && [ "$DE_COUNT" -eq 0 ]; then
        OBSERVED_VIEW=base
    elif [ "$CE_COUNT" -eq 1 ] &&
        [ "$DE_COUNT" -eq 1 ] &&
        [ -n "$CE_SLOT" ] &&
        [ "$CE_SLOT" = "$DE_SLOT" ]; then
        OBSERVED_VIEW=$CE_SLOT
    else
        OBSERVED_VIEW=inconsistent
    fi
fi
printf 'ce_mount_count=%s\n' "$CE_COUNT"
printf 'de_mount_count=%s\n' "$DE_COUNT"
printf 'ce_mount_root=%s\n' "$CE_ROOT"
printf 'de_mount_root=%s\n' "$DE_ROOT"
printf 'observed_view=%s\n' "$OBSERVED_VIEW"

printf '%s\n' '--- recent package Runtime log ---'
if [ -f "$LOG_FILE" ]; then
    "$TOYBOX" grep -F "package=$PACKAGE" "$LOG_FILE" 2>/dev/null |
        "$TOYBOX" tail -n 20
else
    printf '%s\n' 'runtime_log=missing'
fi

printf '%s\n' '--- classification ---'
if [ ! -f "$AGGREGATE" ]; then
    RESULT=not_registered
elif [ -z "$CURRENT_UID" ] || [ -z "$CURRENT_APK_PATH" ]; then
    RESULT=current_package_unavailable
elif [ "$IDENTITY_MATCHES" != yes ]; then
    RESULT=package_identity_changed
elif [ "$RUNTIME_STATE" != live ]; then
    RESULT=runtime_not_live
elif [ "$LIFECYCLE" != ready ]; then
    RESULT=pending_runtime_transaction
elif [ "$OBSERVED_VIEW" = inconsistent ]; then
    RESULT=runtime_view_inconsistent
elif [ "$OBSERVED_VIEW" = unknown ]; then
    RESULT=runtime_view_unavailable
elif [ "$ACTIVE_SLOT" != "$OBSERVED_VIEW" ]; then
    RESULT=aggregate_view_mismatch
else
    RESULT=registration_valid_at_probe_time
fi
printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' 'probe_mutations=none'
printf '%s\n' 'No Runtime request, reset, mount, umount, force-stop, or delete was executed.'
printf '%s\n' '=== end ==='
