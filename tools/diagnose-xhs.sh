#!/system/bin/sh

set -u

PACKAGE=com.xingin.xhs
RUNTIME_ROOT=/data/adb/uclone-slices-v2
AGGREGATE=$RUNTIME_ROOT/packages/$PACKAGE/aggregate.json
PID_FILE=$RUNTIME_ROOT/ucloned.pid
CANONICAL_CE=/data/user/0/$PACKAGE
CANONICAL_DE=/data/user_de/0/$PACKAGE
SLOT_CE_ROOT=/data/misc_ce/0/uclone-slices-v2/slots/$PACKAGE
SLOT_DE_ROOT=/data/misc_de/0/uclone-slices-v2/slots/$PACKAGE
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd

slot_from_mount_root() {
    mount_root=$1
    marker=/uclone-slices-v2/slots/$PACKAGE/
    case "$mount_root" in
        *"$marker"*)
            slot=${mount_root#*"$marker"}
            case "$slot" in
                base)
                    printf '%s\n' "$slot"
                    ;;
                slot-*)
                    number=${slot#slot-}
                    case "$number" in
                        ''|*[!0-9]*) ;;
                        *) printf '%s\n' "$slot" ;;
                    esac
                    ;;
            esac
            ;;
    esac
}

package_line_from_output() {
    package_output=$1
    while IFS= read -r package_line; do
        case "$package_line" in
            "package:$PACKAGE uid:"*)
                printf '%s\n' "$package_line"
                return
                ;;
        esac
    done <<EOF
$package_output
EOF
}

base_apk_from_output() {
    path_output=$1
    while IFS= read -r path_line; do
        case "$path_line" in
            package:*/base.apk)
                printf '%s\n' "${path_line#package:}"
                return
                ;;
        esac
    done <<EOF
$path_output
EOF
}

launcher_from_output() {
    resolve_output=$1
    launcher_component=
    while IFS= read -r resolve_line; do
        case "$resolve_line" in
            "$PACKAGE/"?*) launcher_component=$resolve_line ;;
        esac
    done <<EOF
$resolve_output
EOF
    printf '%s\n' "$launcher_component"
}

mount_roots_for() {
    mountinfo_file=$1
    mount_point=$2
    while IFS= read -r mount_line; do
        set -- $mount_line
        if [ "$#" -ge 5 ] && [ "$5" = "$mount_point" ]; then
            printf '%s\n' "$4"
        fi
    done <"$mountinfo_file"
}

count_roots() {
    roots=$1
    root_count=0
    for root_value in $roots; do
        root_count=$((root_count + 1))
    done
    printf '%s\n' "$root_count"
}

first_root() {
    roots=$1
    for root_value in $roots; do
        printf '%s\n' "$root_value"
        return
    done
}

print_mount_lines() {
    mountinfo_file=$1
    mount_point=$2
    while IFS= read -r mount_line; do
        set -- $mount_line
        if [ "$#" -ge 5 ] && [ "$5" = "$mount_point" ]; then
            printf '%s\n' "$mount_line"
        fi
    done <"$mountinfo_file"
}

classify_result() {
    if [ ! -f "$AGGREGATE" ]; then
        printf '%s\n' aggregate_missing
    elif [ -z "$ACTIVE_SLOT" ] || [ -z "$LIFECYCLE" ] ||
        [ -z "$STORED_UID" ] || [ -z "$STORED_APK_PATH" ] ||
        [ -z "$STORED_APK_DEVICE" ] || [ -z "$STORED_APK_INODE" ]; then
        printf '%s\n' aggregate_unreadable_or_invalid
    elif [ "$PACKAGE_LIST_STATUS" -ne 0 ] || [ -z "$CURRENT_UID" ]; then
        printf '%s\n' package_not_in_runtime_third_party_filter
    elif [ "$APK_STATUS" -ne 0 ] || [ -z "$CURRENT_APK_PATH" ] ||
        [ "$APK_STAT_STATUS" -ne 0 ]; then
        printf '%s\n' package_base_apk_unavailable
    elif [ "$LAUNCHER_STATUS" -ne 0 ] || [ -z "$LAUNCHER_COMPONENT" ]; then
        printf '%s\n' package_launcher_unavailable
    elif [ "$CE_DIRECTORY" != present ] || [ "$DE_DIRECTORY" != present ]; then
        printf '%s\n' package_ce_de_directory_incomplete
    elif [ "$STORED_UID" != "$CURRENT_UID" ] ||
        [ "$STORED_APK_PATH" != "$CURRENT_APK_PATH" ] ||
        [ "$STORED_APK_DEVICE" != "$CURRENT_APK_DEVICE" ] ||
        [ "$STORED_APK_INODE" != "$CURRENT_APK_INODE" ]; then
        printf '%s\n' package_identity_changed
    elif [ "$LIFECYCLE" != ready ]; then
        printf '%s\n' pending_transaction
    elif [ "$RUNTIME_STATE" != live ] || [ ! -r "$MOUNTINFO" ]; then
        printf '%s\n' runtime_mount_view_unavailable
    elif [ "$ACTIVE_SLOT" = base ] &&
        [ "$CE_MOUNT_COUNT" -eq 0 ] && [ "$DE_MOUNT_COUNT" -eq 0 ]; then
        printf '%s\n' aggregate_matches_base
    elif [ "$ACTIVE_SLOT" != base ] &&
        [ "$CE_MOUNT_COUNT" -eq 0 ] && [ "$DE_MOUNT_COUNT" -eq 0 ]; then
        printf '%s\n' persisted_slot_but_runtime_is_base
    elif [ "$CE_MOUNT_COUNT" -ne 1 ] || [ "$DE_MOUNT_COUNT" -ne 1 ]; then
        printf '%s\n' ce_de_mount_count_inconsistent
    elif [ -z "$CE_MOUNT_SLOT" ] || [ -z "$DE_MOUNT_SLOT" ]; then
        printf '%s\n' non_uclone_or_invalid_mount_root
    elif [ "$CE_MOUNT_SLOT" != "$DE_MOUNT_SLOT" ]; then
        printf '%s\n' ce_de_mounted_to_different_slots
    elif [ "$CE_MOUNT_SLOT" != "$ACTIVE_SLOT" ]; then
        printf '%s\n' aggregate_runtime_view_mismatch
    elif [ ! -d "$SLOT_CE_ROOT/$ACTIVE_SLOT" ] ||
        [ ! -d "$SLOT_DE_ROOT/$ACTIVE_SLOT" ]; then
        printf '%s\n' active_slot_pair_incomplete
    else
        printf '%s\n' aggregate_matches_runtime_slot
    fi
}

printf '%s\n' '=== UClone Slices V2 XHS diagnosis ==='
printf '%s\n' 'script_version=2'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'package=%s\n' "$PACKAGE"
printf 'date=%s\n' "$(date 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    printf '%s\n' 'Run this script from a root shell.'
    exit 2
fi

printf '%s\n' '--- persisted aggregate ---'
ACTIVE_SLOT=
LIFECYCLE=
STORED_UID=
STORED_APK_PATH=
STORED_APK_DEVICE=
STORED_APK_INODE=
if [ -f "$AGGREGATE" ]; then
    printf 'aggregate_path=%s\n' "$AGGREGATE"
    printf 'aggregate_json='
    "$TOYBOX_BIN" cat "$AGGREGATE" 2>&1
    printf '\n'
    ACTIVE_SLOT=$(
        "$TOYBOX_BIN" sed -n 's/.*"active_slot":"\([^"]*\)".*/\1/p' "$AGGREGATE"
    )
    LIFECYCLE=$(
        "$TOYBOX_BIN" sed -n 's/.*"lifecycle":{"state":"\([^"]*\)".*/\1/p' "$AGGREGATE"
    )
    STORED_UID=$(
        "$TOYBOX_BIN" sed -n 's/.*"uid":\([0-9][0-9]*\).*/\1/p' "$AGGREGATE"
    )
    STORED_APK_PATH=$(
        "$TOYBOX_BIN" sed -n 's|.*"apk_path":"\([^"]*\)".*|\1|p' "$AGGREGATE"
    )
    STORED_APK_DEVICE=$(
        "$TOYBOX_BIN" sed -n 's/.*"apk_device":\([0-9][0-9]*\).*/\1/p' "$AGGREGATE"
    )
    STORED_APK_INODE=$(
        "$TOYBOX_BIN" sed -n 's/.*"apk_inode":\([0-9][0-9]*\).*/\1/p' "$AGGREGATE"
    )
    printf 'stored_active_slot=%s\n' "$ACTIVE_SLOT"
    printf 'stored_lifecycle=%s\n' "$LIFECYCLE"
    printf 'stored_uid=%s\n' "$STORED_UID"
    printf 'stored_apk_path=%s\n' "$STORED_APK_PATH"
    printf 'stored_apk_device=%s\n' "$STORED_APK_DEVICE"
    printf 'stored_apk_inode=%s\n' "$STORED_APK_INODE"
else
    printf 'aggregate_path=%s\n' "$AGGREGATE"
    printf '%s\n' 'aggregate=missing'
fi

printf '%s\n' '--- current package inspection ---'
PACKAGE_LIST_OUTPUT=$(
    "$CMD_BIN" package list packages -3 -U --user 0 2>&1
)
PACKAGE_LIST_STATUS=$?
PACKAGE_LINE=$(package_line_from_output "$PACKAGE_LIST_OUTPUT")
CURRENT_UID=
UID_PREFIX="package:$PACKAGE uid:"
case "$PACKAGE_LINE" in
    "$UID_PREFIX"*)
        CURRENT_UID=${PACKAGE_LINE#"$UID_PREFIX"}
        case "$CURRENT_UID" in
            ''|*[!0-9]*) CURRENT_UID= ;;
        esac
        ;;
esac
printf 'third_party_list_exit=%s\n' "$PACKAGE_LIST_STATUS"
printf 'third_party_line=%s\n' "$PACKAGE_LINE"
printf 'current_uid=%s\n' "$CURRENT_UID"

APK_OUTPUT=$(
    "$CMD_BIN" package path --user 0 "$PACKAGE" 2>&1
)
APK_STATUS=$?
CURRENT_APK_PATH=$(
    base_apk_from_output "$APK_OUTPUT"
)
CURRENT_APK_DEVICE=
CURRENT_APK_INODE=
APK_STAT_STATUS=1
APK_STAT_OUTPUT=
if [ -n "$CURRENT_APK_PATH" ]; then
    APK_STAT_OUTPUT=$(
        "$TOYBOX_BIN" stat -c '%d %i' "$CURRENT_APK_PATH" 2>&1
    )
    APK_STAT_STATUS=$?
    if [ "$APK_STAT_STATUS" -eq 0 ]; then
        CURRENT_APK_DEVICE=${APK_STAT_OUTPUT%% *}
        CURRENT_APK_INODE=${APK_STAT_OUTPUT#* }
    fi
fi
printf 'package_path_exit=%s\n' "$APK_STATUS"
printf 'package_path_output=%s\n' "$APK_OUTPUT"
printf 'current_apk_path=%s\n' "$CURRENT_APK_PATH"
printf 'apk_stat_exit=%s\n' "$APK_STAT_STATUS"
printf 'apk_stat_output=%s\n' "$APK_STAT_OUTPUT"
printf 'current_apk_device=%s\n' "$CURRENT_APK_DEVICE"
printf 'current_apk_inode=%s\n' "$CURRENT_APK_INODE"

LAUNCHER_OUTPUT=$(
    "$CMD_BIN" package resolve-activity --brief --user 0 \
        -a android.intent.action.MAIN \
        -c android.intent.category.LAUNCHER \
        "$PACKAGE" 2>&1
)
LAUNCHER_STATUS=$?
LAUNCHER_COMPONENT=$(
    launcher_from_output "$LAUNCHER_OUTPUT"
)
printf 'launcher_exit=%s\n' "$LAUNCHER_STATUS"
printf 'launcher_output=%s\n' "$LAUNCHER_OUTPUT"
printf 'launcher_component=%s\n' "$LAUNCHER_COMPONENT"

if [ -d "$CANONICAL_CE" ]; then
    CE_DIRECTORY=present
else
    CE_DIRECTORY=missing
fi
if [ -d "$CANONICAL_DE" ]; then
    DE_DIRECTORY=present
else
    DE_DIRECTORY=missing
fi
printf 'canonical_ce=%s\n' "$CE_DIRECTORY"
ls -ld "$CANONICAL_CE" 2>&1
printf 'canonical_de=%s\n' "$DE_DIRECTORY"
ls -ld "$CANONICAL_DE" 2>&1

printf '%s\n' '--- stored slot directories ---'
printf 'ce_slot_root=%s\n' "$SLOT_CE_ROOT"
ls -la "$SLOT_CE_ROOT" 2>&1
printf 'de_slot_root=%s\n' "$SLOT_DE_ROOT"
ls -la "$SLOT_DE_ROOT" 2>&1

printf '%s\n' '--- Runtime mount view ---'
RUNTIME_PID=
RUNTIME_STATE=missing
MOUNTINFO=
if [ -f "$PID_FILE" ]; then
    RUNTIME_PID=$("$TOYBOX_BIN" cat "$PID_FILE" 2>/dev/null)
    case "$RUNTIME_PID" in
        ''|*[!0-9]*) RUNTIME_STATE=invalid_pid ;;
        *)
            if kill -0 "$RUNTIME_PID" 2>/dev/null; then
                RUNTIME_STATE=live
                MOUNTINFO=/proc/$RUNTIME_PID/mountinfo
            else
                RUNTIME_STATE=stale_pid
            fi
            ;;
    esac
fi
printf 'runtime_pid=%s\n' "$RUNTIME_PID"
printf 'runtime_state=%s\n' "$RUNTIME_STATE"
printf 'runtime_mount_ns=%s\n' "$(readlink "/proc/$RUNTIME_PID/ns/mnt" 2>/dev/null)"
printf 'init_mount_ns=%s\n' "$(readlink /proc/1/ns/mnt 2>/dev/null)"

CE_MOUNT_COUNT=0
DE_MOUNT_COUNT=0
CE_MOUNT_ROOT=
DE_MOUNT_ROOT=
CE_MOUNT_SLOT=
DE_MOUNT_SLOT=
if [ "$RUNTIME_STATE" = live ] && [ -r "$MOUNTINFO" ]; then
    CE_MOUNT_ROOTS=$(mount_roots_for "$MOUNTINFO" "$CANONICAL_CE")
    DE_MOUNT_ROOTS=$(mount_roots_for "$MOUNTINFO" "$CANONICAL_DE")
    CE_MOUNT_COUNT=$(count_roots "$CE_MOUNT_ROOTS")
    DE_MOUNT_COUNT=$(count_roots "$DE_MOUNT_ROOTS")
    CE_MOUNT_ROOT=$(first_root "$CE_MOUNT_ROOTS")
    DE_MOUNT_ROOT=$(first_root "$DE_MOUNT_ROOTS")
    CE_MOUNT_SLOT=$(slot_from_mount_root "$CE_MOUNT_ROOT")
    DE_MOUNT_SLOT=$(slot_from_mount_root "$DE_MOUNT_ROOT")

    printf '%s\n' 'ce_mountinfo_lines:'
    print_mount_lines "$MOUNTINFO" "$CANONICAL_CE"
    printf '%s\n' 'de_mountinfo_lines:'
    print_mount_lines "$MOUNTINFO" "$CANONICAL_DE"
fi
printf 'ce_mount_count=%s\n' "$CE_MOUNT_COUNT"
printf 'de_mount_count=%s\n' "$DE_MOUNT_COUNT"
printf 'ce_mount_root=%s\n' "$CE_MOUNT_ROOT"
printf 'de_mount_root=%s\n' "$DE_MOUNT_ROOT"
printf 'ce_mount_slot=%s\n' "$CE_MOUNT_SLOT"
printf 'de_mount_slot=%s\n' "$DE_MOUNT_SLOT"

printf '%s\n' '--- running App process views ---'
APP_PIDS=$(/system/bin/pidof "$PACKAGE" 2>/dev/null)
printf 'app_pids=%s\n' "$APP_PIDS"
for app_pid in $APP_PIDS; do
    case "$app_pid" in
        ''|*[!0-9]*) continue ;;
    esac
    app_mountinfo=/proc/$app_pid/mountinfo
    printf 'app_pid=%s mount_ns=%s\n' \
        "$app_pid" "$(readlink "/proc/$app_pid/ns/mnt" 2>/dev/null)"
    if [ -r "$app_mountinfo" ]; then
        print_mount_lines "$app_mountinfo" "$CANONICAL_CE"
        print_mount_lines "$app_mountinfo" "$CANONICAL_DE"
    fi
done

printf '%s\n' '--- classification ---'
RESULT=$(classify_result)

printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' 'rpc_invoked=no'
printf '%s\n' 'No Runtime request was sent, so pending recovery was not triggered.'
printf '%s\n' '=== end ==='
