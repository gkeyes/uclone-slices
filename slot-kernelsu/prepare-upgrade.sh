#!/system/bin/sh

set -u
umask 077

RUNTIME_ROOT=/data/adb/uclone-slices-preview
INSTALLED_MODULE=/data/adb/modules/uclone-slices-preview
TOYBOX_BIN=/system/bin/toybox
PROOF_ROOT=$RUNTIME_ROOT/upgrade
PROOF_FILE=$PROOF_ROOT/base-proof
BOOT_ID_FILE=/proc/sys/kernel/random/boot_id
UPTIME_FILE=/proc/uptime
MAX_PROOF_AGE=600
CONTROL_ROOTS='enrollment compatibility-policy catalog registry package-state slot-metadata enrollment-attempts rescue-journal journal state'

fail() {
    printf 'UClone Slots upgrade proof: %s\n' "$1" >&2
    exit 1
}

safe_toybox() {
    [ -f "$TOYBOX_BIN" ] && [ ! -L "$TOYBOX_BIN" ] && [ -x "$TOYBOX_BIN" ] || return 1
    owner="$($TOYBOX_BIN stat -c '%u' "$TOYBOX_BIN" 2>/dev/null)" || return 1
    [ "$owner" = 0 ]
}

safe_slotctl() {
    binary=$INSTALLED_MODULE/bin/slotctl
    [ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] || return 1
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$binary" 2>/dev/null)" || return 1
    case "$metadata" in 0:0:700|0:0:500) return 0 ;; *) return 1 ;; esac
}

prepare_proof_root() {
    if [ -L "$PROOF_ROOT" ] || { [ -e "$PROOF_ROOT" ] && [ ! -d "$PROOF_ROOT" ]; }; then
        return 1
    fi
    "$TOYBOX_BIN" mkdir -p "$PROOF_ROOT" 2>/dev/null || return 1
    "$TOYBOX_BIN" chmod 700 "$PROOF_ROOT" 2>/dev/null || return 1
    "$TOYBOX_BIN" chown 0:0 "$PROOF_ROOT" 2>/dev/null || return 1
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$PROOF_ROOT" 2>/dev/null)" = 0:0:700 ]
}

safe_proof_root() {
    [ -d "$PROOF_ROOT" ] && [ ! -L "$PROOF_ROOT" ] || return 1
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$PROOF_ROOT" 2>/dev/null)" = 0:0:700 ]
}

active_gate_present() {
    state_root=$RUNTIME_ROOT/state
    [ -d "$state_root" ] && [ ! -L "$state_root" ] || return 0
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$state_root" 2>/dev/null)" = 0:0:700 ] || return 0
    for path in "$state_root"/* "$state_root"/.[!.]* "$state_root"/..?*; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        [ -f "$path" ] && [ ! -L "$path" ] || return 0
        name=${path##*/}
        case "$name" in .*.gate.retired) ;; *) return 0 ;; esac
    done
    return 1
}

record_control_path() {
    path=$1
    relative=${path#"$RUNTIME_ROOT"/}
    case "$relative" in *'|'*|'') return 1 ;; esac
    [ ! -L "$path" ] || return 1
    if [ -d "$path" ]; then
        [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$path" 2>/dev/null)" = 0:0:700 ] || return 1
        printf 'D|%s\n' "$relative" >>"$DIGEST_MANIFEST"
        return
    fi
    [ -f "$path" ] || return 1
    metadata="$($TOYBOX_BIN stat -c '%u:%g:%a' "$path" 2>/dev/null)" || return 1
    case "$metadata" in 0:0:600|0:0:400) ;; *) return 1 ;; esac
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
        : >"$temporary" || return 1
        "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || return 1
    done
    result=1
    valid=1
    for name in $CONTROL_ROOTS; do
        root=$RUNTIME_ROOT/$name
        if [ ! -e "$root" ] && [ ! -L "$root" ]; then
            printf 'M|%s\n' "$name" >>"$DIGEST_MANIFEST" || break
            continue
        fi
        record_control_path "$root" || { valid=0; break; }
        "$TOYBOX_BIN" find "$root" -mindepth 1 -print >>"$DIGEST_LIST" 2>/dev/null || { valid=0; break; }
    done
    if [ "$valid" -eq 1 ] && "$TOYBOX_BIN" sort "$DIGEST_LIST" >"$DIGEST_SORTED" 2>/dev/null; then
        while IFS= read -r path; do
            record_control_path "$path" || { valid=0; break; }
        done <"$DIGEST_SORTED"
        if [ "$valid" -eq 1 ]; then
            line="$($TOYBOX_BIN sha256sum "$DIGEST_MANIFEST" 2>/dev/null)" || line=
            digest=${line%% *}
            if [ "${#digest}" -eq 64 ]; then
                case "$digest" in *[!0-9a-f]*) ;; *) printf '%s\n' "$digest"; result=0 ;; esac
            fi
        fi
    fi
    "$TOYBOX_BIN" rm -f "$DIGEST_LIST" "$DIGEST_SORTED" "$DIGEST_MANIFEST" 2>/dev/null || result=1
    return "$result"
}

managed_base_count() {
    frame="$($TOYBOX_BIN timeout -s 9 30 "$INSTALLED_MODULE/bin/slotctl" apps 2>/dev/null)" || return 1
    case "$frame" in *'"status":"ok"'*'"kind":"managed_apps"'*'"apps":['*) ;; *) return 1 ;; esac
    slots="$(printf '%s\n' "$frame" | "$TOYBOX_BIN" grep -o '"active_slot":"[^"]*"' 2>/dev/null)"
    states="$(printf '%s\n' "$frame" | "$TOYBOX_BIN" grep -o '"lifecycle":"[^"]*"' 2>/dev/null)"
    [ -n "$slots" ] && [ -n "$states" ] || return 1
    [ -z "$(printf '%s\n' "$slots" | "$TOYBOX_BIN" grep -F -v '"active_slot":"base"' 2>/dev/null)" ] || return 1
    [ -z "$(printf '%s\n' "$states" | "$TOYBOX_BIN" grep -F -v '"lifecycle":"normal"' 2>/dev/null)" ] || return 1
    slot_count="$(printf '%s\n' "$slots" | "$TOYBOX_BIN" wc -l | "$TOYBOX_BIN" tr -d '[:space:]')"
    state_count="$(printf '%s\n' "$states" | "$TOYBOX_BIN" wc -l | "$TOYBOX_BIN" tr -d '[:space:]')"
    [ "$slot_count" = "$state_count" ] && [ "$slot_count" -ge 1 ] && [ "$slot_count" -le 64 ] || return 1
    printf '%s\n' "$slot_count"
}

boot_values() {
    BOOT_ID="$($TOYBOX_BIN cat "$BOOT_ID_FILE" 2>/dev/null)" || return 1
    case "$BOOT_ID" in *[!A-Za-z0-9-]*|'') return 1 ;; esac
    read -r uptime _ <"$UPTIME_FILE" || return 1
    UPTIME_SECONDS=${uptime%%.*}
    case "$UPTIME_SECONDS" in *[!0-9]*|'') return 1 ;; esac
}

verify_proof() {
    safe_proof_root || return 1
    [ -f "$PROOF_FILE" ] && [ ! -L "$PROOF_FILE" ] || return 1
    [ "$($TOYBOX_BIN stat -c '%u:%g:%a' "$PROOF_FILE" 2>/dev/null)" = 0:0:600 ] || return 1
    [ "$($TOYBOX_BIN wc -l <"$PROOF_FILE" | "$TOYBOX_BIN" tr -d '[:space:]')" = 1 ] || return 1
    IFS='|' read -r schema boot issued count expected extra <"$PROOF_FILE" || return 1
    [ "$schema" = 1 ] && [ -z "$extra" ] || return 1
    case "$issued" in *[!0-9]*|'') return 1 ;; esac
    case "$count" in *[!0-9]*|'') return 1 ;; esac
    [ "${#issued}" -le 12 ] || return 1
    [ "$count" -ge 1 ] && [ "$count" -le 64 ] || return 1
    [ "${#expected}" -eq 64 ] || return 1
    case "$expected" in *[!0-9a-f]*|'') return 1 ;; esac
    boot_values || return 1
    [ "$boot" = "$BOOT_ID" ] && [ "$UPTIME_SECONDS" -ge "$issued" ] || return 1
    [ $((UPTIME_SECONDS - issued)) -le "$MAX_PROOF_AGE" ] || return 1
    active_gate_present && return 1
    actual="$(control_digest)" || return 1
    [ "$actual" = "$expected" ] || return 1
    printf '%s\n' "$count"
}

prepare_proof() {
    safe_slotctl || fail 'installed Runtime client is unavailable or unsafe'
    active_gate_present && fail 'an active App Gate lease exists'
    count="$(managed_base_count)" || fail 'managed Apps are not all proven Base and normal'
    prepare_proof_root || fail 'cannot prepare the root-only proof directory'
    digest="$(control_digest)" || fail 'control-plane metadata is unsafe or unreadable'
    boot_values || fail 'boot identity is unavailable'
    temporary=$PROOF_ROOT/.base-proof.$$
    [ ! -e "$temporary" ] && [ ! -L "$temporary" ] || fail 'temporary proof path already exists'
    printf '1|%s|%s|%s|%s\n' "$BOOT_ID" "$UPTIME_SECONDS" "$count" "$digest" >"$temporary" || fail 'cannot write proof'
    "$TOYBOX_BIN" chmod 600 "$temporary" 2>/dev/null || fail 'cannot protect proof'
    "$TOYBOX_BIN" chown 0:0 "$temporary" 2>/dev/null || fail 'cannot own proof'
    "$TOYBOX_BIN" mv "$temporary" "$PROOF_FILE" 2>/dev/null || fail 'cannot publish proof'
    printf 'upgrade-proof-ready apps=%s valid_seconds=%s\n' "$count" "$MAX_PROOF_AGE"
}

[ "$($TOYBOX_BIN id -u 2>/dev/null)" = 0 ] || fail 'root is required'
safe_toybox || fail 'trusted toybox is unavailable'
case "${1:---prepare}" in
    --prepare) prepare_proof ;;
    --verify) verify_proof || fail 'proof is missing, stale, changed, or unsafe' ;;
    --consume)
        verify_proof >/dev/null || fail 'proof cannot be consumed safely'
        "$TOYBOX_BIN" rm -f "$PROOF_FILE" 2>/dev/null || fail 'proof removal failed'
        ;;
    *) fail 'usage: prepare-upgrade.sh [--prepare|--verify|--consume]' ;;
esac
