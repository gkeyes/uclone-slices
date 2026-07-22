#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
SOURCE=$REPO_ROOT/slot-kernelsu/customize.sh
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/uclone-upgrade-gate.XXXXXX")
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM

fail() {
    printf 'KernelSU upgrade-gate test failed: %s\n' "$1" >&2
    exit 1
}

make_toybox() {
    target=$1
    cat >"$target" <<'EOF'
#!/bin/sh
command=$1
shift
case "$command" in
    timeout)
        [ "${1:-}" = "-s" ] && shift 2
        shift
        exec "$@"
        ;;
    stat)
        [ "${1:-}" = "-c" ] || exit 2
        case "${2:-}" in
            %u) printf '%s\n' 0 ;;
            %a) printf '%s\n' 700 ;;
            *) exit 2 ;;
        esac
        ;;
    grep|wc|tr) exec "$command" "$@" ;;
    *) exit 127 ;;
esac
EOF
    chmod 0700 "$target"
}

make_slotctl() {
    target=$1
    reconcile_status=$2
    apps_status=$3
    apps_frame=$4
    cat >"$target" <<EOF
#!/bin/sh
case "\${1:-}" in
    reconcile) exit $reconcile_status ;;
    apps)
        printf '%s\\n' '$apps_frame'
        exit $apps_status
        ;;
    *) exit 2 ;;
esac
EOF
    chmod 0700 "$target"
}

run_case() {
    name=$1
    expected=$2
    artifact=$3
    gate=$4
    reconcile_status=$5
    apps_status=$6
    apps_frame=$7
    proof=${8:-no}
    case_root=$SCRATCH/$name
    runtime=$case_root/runtime
    installed=$case_root/installed
    staging=$case_root/staging
    mkdir -p "$runtime" "$installed/bin" "$staging/bin"
    make_toybox "$case_root/toybox"
    make_slotctl "$installed/bin/slotctl" "$reconcile_status" "$apps_status" "$apps_frame"
    case "$proof" in
        valid)
            cat >"$staging/prepare-upgrade.sh" <<'EOF'
#!/bin/sh
case "${1:-}" in
    --verify) printf '%s\n' 1 ;;
    --consume) : ;;
    *) exit 2 ;;
esac
EOF
            chmod 0700 "$staging/prepare-upgrade.sh"
            ;;
        no) ;;
        *) fail "unknown proof fixture $proof" ;;
    esac
    if [ "$artifact" = yes ]; then
        mkdir -p "$runtime/enrollment/packages/com.asksky.fitness"
        : >"$runtime/enrollment/packages/com.asksky.fitness/enrollment.json"
    fi
    case "$gate" in
        active)
            mkdir -p "$runtime/state"
            : >"$runtime/state/com.asksky.fitness.gate"
            ;;
        retired)
            mkdir -p "$runtime/state"
            : >"$runtime/state/.com.asksky.fitness.gate.retired"
            ;;
        no) ;;
        *) fail "unknown gate fixture $gate" ;;
    esac
    sed \
        -e "s|^RUNTIME_ROOT=.*|RUNTIME_ROOT=$runtime|" \
        -e "s|^INSTALLED_MODULE=.*|INSTALLED_MODULE=$installed|" \
        -e "s|^TOYBOX_BIN=.*|TOYBOX_BIN=$case_root/toybox|" \
        "$SOURCE" >"$case_root/customize.sh"
    set +e
    MODPATH=$staging /bin/sh -c '
        ui_print() { :; }
        abort() { exit 73; }
        set_perm() { :; }
        set_perm_recursive() { :; }
        . "$1"
    ' shell "$case_root/customize.sh"
    actual=$?
    set -e
    if [ "$expected" = pass ]; then
        [ "$actual" -eq 0 ] || fail "$name returned $actual, expected success"
    else
        [ "$actual" -eq 73 ] || fail "$name returned $actual, expected installer refusal"
    fi
}

empty='{"schema_version":1,"request_id":"test","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[]}}}'
base='{"schema_version":1,"request_id":"test","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[{"package":"com.asksky.fitness","active_slot":"base","lifecycle":"normal"}]}}}'
preview='{"schema_version":1,"request_id":"test","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[{"package":"com.asksky.fitness","active_slot":"slot-1","lifecycle":"normal"}]}}}'
recovery='{"schema_version":1,"request_id":"test","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[{"package":"com.asksky.fitness","active_slot":"base","lifecycle":"recovery_required"}]}}}'

run_case clean pass no no 0 0 "$empty"
run_case base_metadata pass yes no 0 0 "$base"
run_case legacy_reconcile_rejected_but_base_proved pass yes no 1 0 "$base"
run_case retired_gate_evidence pass yes retired 0 0 "$base"
run_case active_preview reject yes no 0 0 "$preview"
run_case recovery_required reject yes no 0 0 "$recovery"
run_case daemon_offline reject yes no 1 1 "$empty"
run_case installer_rpc_unavailable_with_valid_proof pass yes no 1 1 "$empty" valid
run_case orphan_metadata reject yes no 0 0 "$empty"
run_case active_gate reject yes active 0 0 "$base"

printf '%s\n' 'KernelSU paired-upgrade gate scenarios passed.'
