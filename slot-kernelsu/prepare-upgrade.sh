#!/system/bin/sh
set -u
umask 077
RUNTIME_ROOT=/data/adb/uclone-slices-preview
INSTALLED_MODULE=/data/adb/modules/uclone-slices-preview
TOYBOX_BIN=/system/bin/toybox
SYSTEM_SHELL=/system/bin/sh
FD_ROOT=/proc/self/fd
FD_EXEC_INTERPRETER=
case "$0" in */*) SCRIPT_ROOT=${0%/*} ;; *) SCRIPT_ROOT=. ;; esac
FREEZE_HELPER=$SCRIPT_ROOT/upgrade-freeze.sh
STAGED_SLOTCTL=$SCRIPT_ROOT/bin/slotctl
PROOF_ROOT=$RUNTIME_ROOT/upgrade
PROOF_FILE=$PROOF_ROOT/base-proof
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
UPTIME_FILE=/proc/uptime
MAX_PROOF_AGE=600
CONTROL_ROOTS='enrollment compatibility-policy catalog registry package-state slot-metadata enrollment-attempts rescue-journal journal state'
fail() { printf 'UClone Slots upgrade proof: %s\n' "$1" >&2; exit 1; }
safe_toybox() {
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || return 1; [ "$($TOYBOX_BIN stat -c '%u' "$TOYBOX_BIN" 2>/dev/null)" = 0 ]
}
safe_executable() {
    [ -f "$1" ] && [ ! -L "$1" ] && [ -x "$1" ] || return 1
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$1" 2>/dev/null)" || return 1
    case "$metadata" in 0:0:700|0:0:500) return 0 ;; *) return 1 ;; esac
}
file_digest() {
    line="$($TOYBOX_BIN sha256sum "$1" 2>/dev/null)" || return 1
    digest=${line%% *}
    [ "${#digest}" -eq 64 ] || return 1
    case "$digest" in *[!0-9a-f]*|'') return 1 ;; esac
    printf '%s\n' "$digest"
}
pid1_mount_namespace() {
    current="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/self/ns/mnt 2>/dev/null)" || return 1
    pid1="$($TOYBOX_BIN stat -L -c '%d:%i' /proc/1/ns/mnt 2>/dev/null)" || return 1
    case "$current" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    case "$pid1" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "$current" = "$pid1" ]
}
exec_trusted_binary() {
    binary=$1
    shift
    safe_executable "$binary" || return 1
    exec 9<"$binary" || return 1
    descriptor=$FD_ROOT/9
    [ -f "$descriptor" ] && [ -x "$descriptor" ] || return 1
    metadata="$($TOYBOX_BIN stat -L -c '%u:%g:%a' "$descriptor" 2>/dev/null)" || return 1
    case "$metadata" in 0:0:700|0:0:500) ;; *) return 1 ;; esac
    path_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$binary" 2>/dev/null)" || return 1
    descriptor_node="$($TOYBOX_BIN stat -L -c '%d:%i' "$descriptor" 2>/dev/null)" || return 1
    case "$descriptor_node" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "$path_node" = "$descriptor_node" ] || return 1
    path_digest="$(file_digest "$binary")" || return 1
    descriptor_digest="$(file_digest "$descriptor")" || return 1
    [ "$path_digest" = "$descriptor_digest" ] || return 1
    [ -n "$FD_EXEC_INTERPRETER" ] && exec "$FD_EXEC_INTERPRETER" "$descriptor" "$@"
    exec "$descriptor" "$@"
}
safe_root_script() {
    [ -f "$1" ] && [ ! -L "$1" ] || return 1
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$1" 2>/dev/null)" || return 1; case "$metadata" in 0:0:700|0:0:600|0:0:500|0:0:400) return 0 ;; *) return 1 ;; esac
}
safe_script_root() {
    [ -d "$SCRIPT_ROOT" ] && [ ! -L "$SCRIPT_ROOT" ] || return 1
    safe_control_metadata "$SCRIPT_ROOT"
}
prepare_proof_root() {
    if [ -L "$PROOF_ROOT" ] || { [ -e "$PROOF_ROOT" ] && [ ! -d "$PROOF_ROOT" ]; }; then return 1; fi
    "$TOYBOX_BIN" mkdir -p "$PROOF_ROOT" 2>/dev/null && "$TOYBOX_BIN" chmod 700 "$PROOF_ROOT" 2>/dev/null && "$TOYBOX_BIN" chown 0:0 "$PROOF_ROOT" 2>/dev/null || return 1
    safe_proof_root
}
safe_proof_root() {
    [ -d "$PROOF_ROOT" ] && [ ! -L "$PROOF_ROOT" ] || return 1; [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$PROOF_ROOT" 2>/dev/null)" = 0:0:700 ]
}
active_gate_present() {
    state_root=$RUNTIME_ROOT/state
    [ -d "$state_root" ] && [ ! -L "$state_root" ] || return 0
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$state_root" 2>/dev/null)" = 0:0:700 ] || return 0
    for path in "$state_root"/* "$state_root"/.[!.]* "$state_root"/..?*; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        [ -f "$path" ] && [ ! -L "$path" ] || return 0
        case "${path##*/}" in .*.gate.retired) ;; *) return 0 ;; esac
    done
    return 1
}
safe_control_metadata() {
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$1" 2>/dev/null)" || return 1; case "$metadata" in 0:0:*) mode=${metadata##*:} ;; *) return 1 ;; esac
    case "$mode" in *[!0-7]*|'') return 1 ;; esac
    [ $(((mode / 10) % 10 & 2)) -eq 0 ] && [ $((mode % 10 & 2)) -eq 0 ]
}
record_control_path() {
    path=$1
    relative=${path#"$RUNTIME_ROOT"/}
    case "$relative" in *'|'*|'') return 1 ;; esac
    [ ! -L "$path" ] && safe_control_metadata "$path" || return 1
    if [ -d "$path" ]; then printf 'D|%s\n' "$relative" >>"$DIGEST_MANIFEST"; return; fi
    [ -f "$path" ] || return 1
    line="$($TOYBOX_BIN sha256sum "$path" 2>/dev/null)" || return 1
    digest=${line%% *}
    [ "${#digest}" -eq 64 ] || return 1
    case "$digest" in *[!0-9a-f]*|'') return 1 ;; esac
    printf 'F|%s|%s\n' "$digest" "$relative" >>"$DIGEST_MANIFEST"
}
control_digest() {
    safe_proof_root || return 1
    DIGEST_LIST=$PROOF_ROOT/.digest-list.$$
    DIGEST_SORTED=$PROOF_ROOT/.digest-sorted.$$
    DIGEST_MANIFEST=$PROOF_ROOT/.digest-manifest.$$
    for temporary in "$DIGEST_LIST" "$DIGEST_SORTED" "$DIGEST_MANIFEST"; do
        [ ! -e "$temporary" ] && [ ! -L "$temporary" ] || return 1
        : >"$temporary" && "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    done
    valid=1
    for name in $CONTROL_ROOTS; do
        root=$RUNTIME_ROOT/$name
        if [ ! -e "$root" ] && [ ! -L "$root" ]; then printf 'M|%s\n' "$name" >>"$DIGEST_MANIFEST"; continue; fi
        record_control_path "$root" || { valid=0; break; }
        "$TOYBOX_BIN" find "$root" -mindepth 1 -print >>"$DIGEST_LIST" 2>/dev/null || { valid=0; break; }
    done
    if [ "$valid" -eq 1 ] && "$TOYBOX_BIN" sort "$DIGEST_LIST" >"$DIGEST_SORTED" 2>/dev/null; then while IFS= read -r path; do record_control_path "$path" || { valid=0; break; }; done <"$DIGEST_SORTED"; else valid=0; fi
    result=1
    if [ "$valid" -eq 1 ]; then
        line="$($TOYBOX_BIN sha256sum "$DIGEST_MANIFEST" 2>/dev/null)" || line=
        digest=${line%% *}
        case "$digest" in *[!0-9a-f]*|'') ;; *) [ "${#digest}" -eq 64 ] && { printf '%s\n' "$digest"; result=0; } ;; esac
    fi
    "$TOYBOX_BIN" rm -f "$DIGEST_LIST" "$DIGEST_SORTED" "$DIGEST_MANIFEST" 2>/dev/null || result=1
    return "$result"
}
managed_base_count() {
    count="$($TOYBOX_BIN timeout -s 9 30 "$TOYBOX_BIN" nsenter -t 1 -m -- "$SYSTEM_SHELL" "$SCRIPT_ROOT/prepare-upgrade.sh" --pid1-upgrade-readiness 2>/dev/null)" || return 1
    [ "$(printf '%s\n' "$count" | "$TOYBOX_BIN" wc -l | "$TOYBOX_BIN" tr -d '[:space:]')" = 1 ] || return 1
    case "$count" in *[!0-9]*|'') return 1 ;; esac
    [ "${#count}" -le 2 ] && [ "$count" -ge 0 ] && [ "$count" -le 64 ] || return 1
    printf '%s\n' "$count"
}
boot_values() {
    BOOT_ID="$($TOYBOX_BIN cat "$BOOT_ID_FILE" 2>/dev/null)" || return 1
    case "$BOOT_ID" in *[!A-Za-z0-9-]*|'') return 1 ;; esac
    read -r uptime _ <"$UPTIME_FILE" || return 1
    UPTIME_SECONDS=${uptime%%.*}
    case "$UPTIME_SECONDS" in *[!0-9]*|'') return 1 ;; esac
}
publish_proof() {
    temporary=$PROOF_ROOT/.base-proof.$$
    [ ! -e "$temporary" ] && [ ! -L "$temporary" ] || return 1
    printf '2|%s|%s|%s|%s|%s|%s|%s|%s\n' "$1" "$BOOT_ID" "$UPTIME_SECONDS" "$2" "$3" "$4" "$5" "$6" >"$temporary" || return 1
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null && "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$PROOF_FILE" 2>/dev/null
}
load_proof() {
    safe_proof_root && [ -f "$PROOF_FILE" ] && [ ! -L "$PROOF_FILE" ] || return 1
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$PROOF_FILE" 2>/dev/null)" = 0:0:600 ] || return 1
    [ "$($TOYBOX_BIN wc -l <"$PROOF_FILE" | "$TOYBOX_BIN" tr -d '[:space:]')" = 1 ] || return 1
    IFS='|' read -r schema PROOF_PHASE PROOF_BOOT PROOF_ISSUED PROOF_COUNT PROOF_DIGEST PROOF_PID PROOF_START PROOF_IMAGE extra <"$PROOF_FILE" || return 1
    [ "$schema" = 2 ] && [ -z "$extra" ] || return 1
    case "$PROOF_PHASE" in preparing|ready|staged) ;; *) return 1 ;; esac
    case "$PROOF_BOOT" in *[!A-Za-z0-9-]*|'') return 1 ;; esac
    for value in "$PROOF_ISSUED" "$PROOF_COUNT" "$PROOF_PID" "$PROOF_START"; do case "$value" in *[!0-9]*|'') return 1 ;; esac; done
    [ "${#PROOF_ISSUED}" -le 12 ] && [ "${#PROOF_COUNT}" -le 2 ] && [ "${#PROOF_PID}" -le 10 ] && [ "${#PROOF_START}" -le 20 ] || return 1
    image_node=${PROOF_IMAGE%:*}
    image_digest=${PROOF_IMAGE##*:}
    [ "$image_node" != "$PROOF_IMAGE" ] || return 1
    case "$image_node" in *[!0-9:]*|:*|*:|*:*:*) return 1 ;; esac
    [ "${#image_digest}" -eq 64 ] || return 1
    case "$image_digest" in *[!0-9a-f]*|'') return 1 ;; esac
    [ "$PROOF_COUNT" -ge 0 ] && [ "$PROOF_COUNT" -le 64 ] && [ "${#PROOF_DIGEST}" -eq 64 ] || return 1
    case "$PROOF_DIGEST" in *[!0-9a-f]*|'') return 1 ;; esac
}
freeze() { "$SYSTEM_SHELL" "$FREEZE_HELPER" "$@"; }
verify_proof() {
    load_proof && [ "$PROOF_PHASE" = ready ] && boot_values || return 1
    [ "$PROOF_BOOT" = "$BOOT_ID" ] && [ "$UPTIME_SECONDS" -ge "$PROOF_ISSUED" ] || return 1
    [ $((UPTIME_SECONDS - PROOF_ISSUED)) -le "$MAX_PROOF_AGE" ] || return 1
    active_gate_present && return 1
    actual="$(control_digest)" || return 1
    [ "$actual" = "$PROOF_DIGEST" ] && freeze --verify "$PROOF_PID" "$PROOF_START" "$PROOF_IMAGE" >/dev/null || return 1
    printf '%s\n' "$PROOF_COUNT"
}
cancel_proof() {
    [ -e "$PROOF_FILE" ] || [ -L "$PROOF_FILE" ] || return 0
    load_proof && boot_values && [ "$PROOF_BOOT" = "$BOOT_ID" ] || return 1
    [ "$PROOF_PHASE" = staged ] && publish_disable_marker || [ "$PROOF_PHASE" != staged ] || return 1
    safe_root_script "$FREEZE_HELPER" && freeze --resume "$PROOF_PID" "$PROOF_START" "$PROOF_IMAGE" >/dev/null || return 1
    "$TOYBOX_BIN" rm -f "$PROOF_FILE" 2>/dev/null
}
publish_disable_marker() {
    marker=$SCRIPT_ROOT/disable
    temporary=$SCRIPT_ROOT/.disable.$$
    [ ! -e "$temporary" ] && [ ! -L "$temporary" ] || return 1
    : >"$temporary" || return 1
    "$TOYBOX_BIN" chmod 644 "$temporary" 2>/dev/null && "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || return 1
    "$TOYBOX_BIN" mv "$temporary" "$marker" 2>/dev/null || return 1
    [ -f "$marker" ] && [ ! -L "$marker" ] && [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$marker" 2>/dev/null)" = 0:0:644 ]
}
stage_proof() {
    verify_proof >/dev/null || return 1
    publish_proof staged "$PROOF_COUNT" "$PROOF_DIGEST" "$PROOF_PID" "$PROOF_START" "$PROOF_IMAGE" && freeze --verify "$PROOF_PID" "$PROOF_START" "$PROOF_IMAGE" >/dev/null
}
clear_prior_boot_proof() {
    [ -e "$PROOF_FILE" ] || [ -L "$PROOF_FILE" ] || return 0
    load_proof && boot_values || return 1
    [ "$PROOF_BOOT" != "$BOOT_ID" ] || return 1
    "$TOYBOX_BIN" rm -f "$PROOF_FILE" 2>/dev/null
}
prepare_proof() {
    safe_root_script "$FREEZE_HELPER" || fail 'daemon freeze helper is unavailable or unsafe'
    active_gate_present && fail 'an active App Gate lease exists'
    prepare_proof_root || fail 'cannot prepare the root-only proof directory'
    clear_prior_boot_proof || fail 'a same-boot or corrupt upgrade proof already exists; verify or cancel it first'
    digest_before="$(control_digest)" || fail 'control-plane metadata is unsafe or unreadable'
    identity="$(freeze --inspect)" || fail 'the old Runtime daemon identity is unavailable'
    count="$(managed_base_count)" || fail 'managed Apps are not all proven Base and normal'
    [ "$(freeze --inspect)" = "$identity" ] || fail 'the old Runtime daemon identity changed during Base verification'
    IFS='|' read -r daemon_pid daemon_start daemon_image extra <<EOF
$identity
EOF
    [ -z "$extra" ] || fail 'the old Runtime daemon identity is malformed'
    boot_values || fail 'boot identity is unavailable'
    publish_proof preparing "$count" "$digest_before" "$daemon_pid" "$daemon_start" "$daemon_image" || fail 'cannot publish freeze intent'
    freeze --stop "$daemon_pid" "$daemon_start" "$daemon_image" >/dev/null || { cancel_proof >/dev/null 2>&1; fail 'the old Runtime daemon could not be frozen safely'; }
    if active_gate_present; then cancel_proof >/dev/null 2>&1 || fail 'a Gate race occurred and daemon resume failed'; fail 'an App Gate lease appeared during verification'; fi
    digest_after="$(control_digest)" || { cancel_proof >/dev/null 2>&1; fail 'metadata became unreadable while the daemon was frozen'; }
    if [ "$digest_after" != "$digest_before" ]; then cancel_proof >/dev/null 2>&1 || fail 'metadata changed and daemon resume failed'; fail 'control-plane metadata changed during Base verification'; fi
    publish_proof ready "$count" "$digest_after" "$daemon_pid" "$daemon_start" "$daemon_image" || { cancel_proof >/dev/null 2>&1; fail 'cannot publish the frozen proof'; }
    printf 'upgrade-proof-ready apps=%s valid_seconds=%s\n' "$count" "$MAX_PROOF_AGE"
}
[ "$($TOYBOX_BIN id -u 2>/dev/null)" = 0 ] || fail 'root is required'
safe_toybox || fail 'trusted toybox is unavailable'
safe_script_root && safe_root_script "$FREEZE_HELPER" || fail 'daemon freeze helper path is unavailable or unsafe'
if [ "${1:-}" = --pid1-upgrade-readiness ]; then
    shift
    [ "$#" -eq 0 ] || fail 'invalid installed Runtime request'
    pid1_mount_namespace || fail 'installed Runtime is not visible in the PID1 mount namespace'
    exec_trusted_binary "$STAGED_SLOTCTL" upgrade-readiness ||
        fail 'staged Runtime client identity changed before execution'
fi
case "${1:---prepare}" in
    --prepare) prepare_proof ;;
    --verify) verify_proof || fail 'proof is missing, stale, changed, unfrozen, or unsafe' ;;
    --stage) stage_proof || fail 'proof cannot be handed to the staged module safely' ;;
    --cancel) cancel_proof || fail 'the exact old daemon could not be resumed safely' ;;
    *) fail 'usage: prepare-upgrade.sh [--prepare|--verify|--stage|--cancel]' ;;
esac
