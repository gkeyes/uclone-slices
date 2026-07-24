#!/system/bin/sh

set -u
set -f

PACKAGE=${1:-com.xingin.xhs}
RUNTIME_ROOT=/data/adb/uclone-slices-v2
AGGREGATE=$RUNTIME_ROOT/packages/$PACKAGE/aggregate.json
PID_FILE=$RUNTIME_ROOT/ucloned.pid
LOG_FILE=$RUNTIME_ROOT/ucloned.log
CANONICAL_CE=/data/user/0/$PACKAGE
CANONICAL_DE=/data/user_de/0/$PACKAGE
SLOT_CE_ROOT=/data/misc_ce/0/uclone-slices-v2/slots/$PACKAGE
SLOT_DE_ROOT=/data/misc_de/0/uclone-slices-v2/slots/$PACKAGE
TOYBOX_BIN=/system/bin/toybox
CMD_BIN=/system/bin/cmd

CE_EXACT_MOUNTS=0
DE_EXACT_MOUNTS=0
CE_CHILD_MOUNTS=0
DE_CHILD_MOUNTS=0
CE_MOUNT_IDS=
DE_MOUNT_IDS=
APP_PROCESS_COUNT=0
HOLDER_COUNT=0
PRINTED_HOLDERS=0
MAX_PRINTED_HOLDERS=80
CURRENT_UID=
LSOF_AVAILABLE=no

json_string() {
    key=$1
    file=$2
    "$TOYBOX_BIN" sed -n "s/.*\"$key\":\"\\([^\"]*\\)\".*/\\1/p" "$file"
}

package_uid() {
    package_output=$(
        "$CMD_BIN" package list packages -3 -U --user 0 2>&1
    )
    package_status=$?
    if [ "$package_status" -ne 0 ]; then
        return
    fi
    while IFS= read -r package_line; do
        case "$package_line" in
            "package:$PACKAGE uid:"*)
                uid=${package_line#"package:$PACKAGE uid:"}
                case "$uid" in
                    ''|*[!0-9]*) ;;
                    *)
                        printf '%s\n' "$uid"
                        return
                        ;;
                esac
                ;;
        esac
    done <<EOF
$package_output
EOF
}

record_mount() {
    mount_line=$1
    set -- $mount_line
    if [ "$#" -lt 6 ]; then
        return
    fi
    mount_id=$1
    mount_point=$5
    case "$mount_point" in
        "$CANONICAL_CE")
            CE_EXACT_MOUNTS=$((CE_EXACT_MOUNTS + 1))
            CE_MOUNT_IDS="$CE_MOUNT_IDS $mount_id"
            ;;
        "$CANONICAL_CE"/*)
            CE_CHILD_MOUNTS=$((CE_CHILD_MOUNTS + 1))
            ;;
        "$CANONICAL_DE")
            DE_EXACT_MOUNTS=$((DE_EXACT_MOUNTS + 1))
            DE_MOUNT_IDS="$DE_MOUNT_IDS $mount_id"
            ;;
        "$CANONICAL_DE"/*)
            DE_CHILD_MOUNTS=$((DE_CHILD_MOUNTS + 1))
            ;;
    esac
}

path_is_package_data() {
    candidate=$1
    case "$candidate" in
        "$CANONICAL_CE"|"$CANONICAL_CE"/*|\
        "$CANONICAL_DE"|"$CANONICAL_DE"/*|\
        "$SLOT_CE_ROOT"|"$SLOT_CE_ROOT"/*|\
        "$SLOT_DE_ROOT"|"$SLOT_DE_ROOT"/*|\
        "/data/data/$PACKAGE"|"/data/data/$PACKAGE"/*|\
        "/data_mirror/data_ce/null/0/$PACKAGE"|\
        "/data_mirror/data_ce/null/0/$PACKAGE"/*|\
        "/data_mirror/data_de/null/0/$PACKAGE"|\
        "/data_mirror/data_de/null/0/$PACKAGE"/*)
            return 0
            ;;
    esac
    return 1
}

record_holder() {
    pid=$1
    process_uid=$2
    process_name=$3
    reference_kind=$4
    reference_value=$5
    HOLDER_COUNT=$((HOLDER_COUNT + 1))
    if [ "$PRINTED_HOLDERS" -lt "$MAX_PRINTED_HOLDERS" ]; then
        printf 'holder_pid=%s uid=%s name=%s ref=%s value=%s\n' \
            "$pid" "$process_uid" "$process_name" \
            "$reference_kind" "$reference_value"
        PRINTED_HOLDERS=$((PRINTED_HOLDERS + 1))
    fi
}

read_process_identity() {
    process_dir=$1
    process_name=
    process_uid=
    while read -r status_key status_value status_rest; do
        case "$status_key" in
            Name:) process_name=$status_value ;;
            Uid:) process_uid=$status_value ;;
        esac
        if [ -n "$process_name" ] && [ -n "$process_uid" ]; then
            break
        fi
    done <"$process_dir/status" 2>/dev/null
    printf '%s|%s\n' "$process_uid" "$process_name"
}

filter_lsof() {
    while IFS= read -r lsof_line; do
        case "$lsof_line" in
            *"$CANONICAL_CE"*|*"$CANONICAL_DE"*|*"$SLOT_CE_ROOT"*|*"$SLOT_DE_ROOT"*|*"/data/data/$PACKAGE"*|*"/data_mirror/data_ce/null/0/$PACKAGE"*|*"/data_mirror/data_de/null/0/$PACKAGE"*)
                printf '%s\n' "$lsof_line"
                ;;
        esac
    done
}

scan_lsof() {
    if ! "$TOYBOX_BIN" lsof --help >/dev/null 2>&1; then
        printf '%s\n' 'lsof_available=no'
        return
    fi
    LSOF_AVAILABLE=yes
    printf '%s\n' 'lsof_available=yes'
    LSOF_MATCHES=$(
        "$TOYBOX_BIN" lsof 2>/dev/null | filter_lsof
    )
    while IFS= read -r lsof_line; do
        [ -n "$lsof_line" ] || continue
        HOLDER_COUNT=$((HOLDER_COUNT + 1))
        if [ "$PRINTED_HOLDERS" -lt "$MAX_PRINTED_HOLDERS" ]; then
            printf 'holder_lsof=%s\n' "$lsof_line"
            PRINTED_HOLDERS=$((PRINTED_HOLDERS + 1))
        fi
    done <<EOF
$LSOF_MATCHES
EOF
}

scan_app_processes() {
    for process_dir in /proc/[0-9]*; do
        [ -d "$process_dir" ] || continue
        pid=${process_dir#/proc/}
        identity=$(read_process_identity "$process_dir")
        process_uid=${identity%%|*}
        process_name=${identity#*|}
        mount_ns=$("$TOYBOX_BIN" readlink "$process_dir/ns/mnt" 2>/dev/null)

        if [ -n "$CURRENT_UID" ] && [ "$process_uid" = "$CURRENT_UID" ]; then
            APP_PROCESS_COUNT=$((APP_PROCESS_COUNT + 1))
            cmdline=$(
                "$TOYBOX_BIN" tr '\000' ' ' \
                    <"$process_dir/cmdline" 2>/dev/null
            )
            printf 'app_pid=%s uid=%s name=%s mount_ns=%s cmdline=%s\n' \
                "$pid" "$process_uid" "$process_name" "$mount_ns" "$cmdline"
        fi

        if [ "$LSOF_AVAILABLE" != yes ]; then
            for reference_kind in cwd root exe; do
                reference_value=$(
                    "$TOYBOX_BIN" readlink \
                        "$process_dir/$reference_kind" 2>/dev/null
                )
                if [ -n "$reference_value" ] &&
                    path_is_package_data "$reference_value"; then
                    record_holder "$pid" "$process_uid" "$process_name" \
                        "$reference_kind" "$reference_value"
                fi
            done
        fi
    done
}

printf '%s\n' '=== UClone Slices V2 unmount-busy probe ==='
printf '%s\n' 'script_version=1'
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'package=%s\n' "$PACKAGE"
printf 'date=%s\n' "$(date 2>/dev/null)"

if [ "$(id -u 2>/dev/null)" != 0 ]; then
    printf '%s\n' 'RESULT=not_root'
    printf '%s\n' 'Run this script from a root shell.'
    exit 2
fi

printf '%s\n' '--- persisted transaction ---'
ACTIVE_SLOT=
LIFECYCLE=
PREVIOUS_SLOT=
TARGET_SLOT=
if [ -f "$AGGREGATE" ]; then
    printf 'aggregate_json='
    "$TOYBOX_BIN" cat "$AGGREGATE" 2>&1
    printf '\n'
    ACTIVE_SLOT=$(json_string active_slot "$AGGREGATE")
    LIFECYCLE=$(json_string state "$AGGREGATE")
    PREVIOUS_SLOT=$(json_string previous "$AGGREGATE")
    TARGET_SLOT=$(json_string target "$AGGREGATE")
else
    printf '%s\n' 'aggregate=missing'
fi
printf 'active_slot=%s\n' "$ACTIVE_SLOT"
printf 'lifecycle=%s\n' "$LIFECYCLE"
printf 'previous_slot=%s\n' "$PREVIOUS_SLOT"
printf 'target_slot=%s\n' "$TARGET_SLOT"

printf '%s\n' '--- Runtime namespace ---'
RUNTIME_PID=
RUNTIME_STATE=missing
RUNTIME_MOUNTINFO=
if [ -f "$PID_FILE" ]; then
    RUNTIME_PID=$("$TOYBOX_BIN" cat "$PID_FILE" 2>/dev/null)
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
    "$("$TOYBOX_BIN" readlink "/proc/$RUNTIME_PID/ns/mnt" 2>/dev/null)"
printf 'init_mount_ns=%s\n' \
    "$("$TOYBOX_BIN" readlink /proc/1/ns/mnt 2>/dev/null)"

printf '%s\n' '--- package mount tree ---'
if [ "$RUNTIME_STATE" = live ] && [ -r "$RUNTIME_MOUNTINFO" ]; then
    while IFS= read -r mount_line; do
        record_mount "$mount_line"
        set -- $mount_line
        if [ "$#" -ge 6 ]; then
            mount_root=$4
            mount_point=$5
            case "$mount_point" in
                "$CANONICAL_CE"|"$CANONICAL_CE"/*|\
                "$CANONICAL_DE"|"$CANONICAL_DE"/*)
                    printf '%s\n' "$mount_line"
                    ;;
                *)
                    case "$mount_root" in
                        *"/uclone-slices-v2/slots/$PACKAGE/"*)
                            printf '%s\n' "$mount_line"
                            ;;
                    esac
                    ;;
            esac
        fi
    done <"$RUNTIME_MOUNTINFO"
else
    printf '%s\n' 'mountinfo=unavailable'
fi
printf 'ce_exact_mounts=%s\n' "$CE_EXACT_MOUNTS"
printf 'de_exact_mounts=%s\n' "$DE_EXACT_MOUNTS"
printf 'ce_child_mounts=%s\n' "$CE_CHILD_MOUNTS"
printf 'de_child_mounts=%s\n' "$DE_CHILD_MOUNTS"
printf 'ce_mount_ids=%s\n' "$CE_MOUNT_IDS"
printf 'de_mount_ids=%s\n' "$DE_MOUNT_IDS"

printf '%s\n' '--- recent package Runtime errors ---'
RECENT_BUSY_COUNT=0
if [ -f "$LOG_FILE" ]; then
    RECENT_ERRORS=$(
        "$TOYBOX_BIN" grep -F \
            "op=activate_slot package=$PACKAGE" "$LOG_FILE" 2>/dev/null |
            "$TOYBOX_BIN" tail -n 20
    )
    printf '%s\n' "$RECENT_ERRORS"
    RECENT_BUSY_COUNT=$(
        printf '%s\n' "$RECENT_ERRORS" |
            "$TOYBOX_BIN" grep -F -c 'Device or resource busy'
    )
else
    printf '%s\n' 'runtime_log=missing'
fi
printf 'recent_busy_count=%s\n' "$RECENT_BUSY_COUNT"

printf '%s\n' '--- processes and path holders ---'
CURRENT_UID=$(package_uid)
printf 'package_uid=%s\n' "$CURRENT_UID"
scan_lsof
scan_app_processes
printf 'app_process_count=%s\n' "$APP_PROCESS_COUNT"
printf 'holder_count=%s\n' "$HOLDER_COUNT"
printf 'printed_holder_count=%s\n' "$PRINTED_HOLDERS"
if [ "$HOLDER_COUNT" -gt "$PRINTED_HOLDERS" ]; then
    printf '%s\n' 'holder_output=truncated'
fi

printf '%s\n' '--- classification ---'
if [ "$RUNTIME_STATE" != live ]; then
    RESULT=runtime_not_live
elif [ ! -f "$AGGREGATE" ]; then
    RESULT=aggregate_missing
elif [ "$RECENT_BUSY_COUNT" -eq 0 ]; then
    RESULT=no_recent_unmount_busy_error
elif [ "$CE_CHILD_MOUNTS" -gt 0 ]; then
    RESULT=ce_child_mount_blocks_unmount
elif [ "$APP_PROCESS_COUNT" -gt 0 ]; then
    RESULT=app_process_survived_force_stop
elif [ "$HOLDER_COUNT" -gt 0 ]; then
    RESULT=process_reference_blocks_unmount
elif [ "$LIFECYCLE" = activating ]; then
    RESULT=pending_activation_without_visible_holder
else
    RESULT=unmount_busy_without_visible_holder
fi
printf 'RESULT=%s\n' "$RESULT"
printf '%s\n' 'probe_mutations=none'
printf '%s\n' 'No Runtime request, force-stop, mount, umount, or delete was executed.'
printf '%s\n' '=== end ==='
