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
    rm)
        target=
        for target do :; done
        exec "$command" "$@" &
        child=$!
        wait "$child" || exit
        if [ "${INTERRUPT_AFTER_ACTIVATION:-no}" = yes ] && [ "${target##*/}" = disable ]; then
            kill -TERM "$PPID"
        fi
        ;;
    chmod) exec "$command" "$@" ;;
    *) exit 127 ;;
esac
EOF
    chmod 0700 "$target"
}

make_upgrade_tool() {
    target=$1
    cat >"$target" <<'EOF'
#!/bin/sh
printf '%s\n' "$1" >>"$UPGRADE_TRACE"
case "$1" in
    --verify)
        if [ -f "$UPGRADE_STATE/ready" ]; then
            printf '%s\n' "${PROOF_COUNT:-1}"
            exit 0
        fi
        exit 1
        ;;
    --prepare)
        if [ "${PREPARE_RESULT:-pass}" = freeze_fail ]; then
            : >"$UPGRADE_STATE/ready"
            exit 1
        fi
        [ "${PREPARE_RESULT:-pass}" = pass ] || exit 1
        : >"$UPGRADE_STATE/ready"
        printf '%s\n' 'upgrade-proof-ready apps=1 valid_seconds=600'
        ;;
    --stage)
        [ "${CONSUME_RESULT:-pass}" = pass ] || exit 1
        : >"$UPGRADE_STATE/staged"
        ;;
    --cancel)
        if [ -f "$UPGRADE_STATE/staged" ]; then
            [ -f "$STAGED_DISABLE" ] || exit 1
        fi
        rm -f "$UPGRADE_STATE/ready" "$UPGRADE_STATE/staged"
        ;;
    *) exit 2 ;;
esac
EOF
    chmod 0700 "$target"
}

run_case() {
    name=$1
    expected=$2
    metadata=$3
    preproof=$4
    prepare_result=$5
    consume_result=$6
    expected_trace=$7
    expected_frozen=$8
    proof_count=$9
    interrupt=${10}
    activation_interrupt=${11}
    commit_interrupt=${12}
    case_root=$SCRATCH/$name
    runtime=$case_root/runtime
    installed=$case_root/installed
    staging=$case_root/staging
    state=$case_root/upgrade-state
    mkdir -p "$runtime" "$installed/bin" "$staging" "$state"
    make_toybox "$case_root/toybox"
    make_upgrade_tool "$staging/prepare-upgrade.sh"
    printf '%s\n' '#!/bin/sh' >"$staging/upgrade-freeze.sh"
    chmod 0700 "$staging/upgrade-freeze.sh"
    : >"$staging/disable"
    chmod 0644 "$staging/disable"
    case "$metadata" in
        yes) mkdir -p "$runtime/enrollment/packages/com.asksky.fitness"; : >"$runtime/enrollment/packages/com.asksky.fitness/enrollment.json" ;;
        active) mkdir -p "$runtime/state"; : >"$runtime/state/com.asksky.fitness.gate" ;;
        retired) mkdir -p "$runtime/state"; : >"$runtime/state/.com.asksky.fitness.gate.retired" ;;
        no) ;;
        *) fail "unknown metadata fixture $metadata" ;;
    esac
    [ "$preproof" = yes ] && : >"$state/ready"
    sed \
        -e "s|^RUNTIME_ROOT=.*|RUNTIME_ROOT=$runtime|" \
        -e "s|^INSTALLED_MODULE=.*|INSTALLED_MODULE=$installed|" \
        -e "s|^TOYBOX_BIN=.*|TOYBOX_BIN=$case_root/toybox|" \
        -e 's|^SYSTEM_SHELL=.*|SYSTEM_SHELL=/bin/sh|' \
        "$SOURCE" >"$case_root/customize.sh"
    if [ "$commit_interrupt" = yes ]; then
        awk '
            pending && /UPGRADE_PROOF_HELD=0/ { print "    kill -TERM $$" }
            { print; pending = ($0 == "    trap '\''\'\'' 0 HUP INT TERM") }
        ' "$case_root/customize.sh" >"$case_root/customize.injected"
        mv "$case_root/customize.injected" "$case_root/customize.sh"
    fi
    : >"$case_root/trace"
    set +e
    UPGRADE_TRACE=$case_root/trace \
        UPGRADE_STATE=$state \
        PREPARE_RESULT=$prepare_result \
        CONSUME_RESULT=$consume_result \
        PROOF_COUNT=$proof_count \
        INTERRUPT_AFTER_PROOF=$interrupt \
        INTERRUPT_AFTER_ACTIVATION=$activation_interrupt \
        STAGED_DISABLE=$staging/disable \
        MODPATH=$staging /bin/sh -c '
            ui_print() { :; }
            abort() { exit 73; }
            set_perm() { :; }
            set_perm_recursive() { [ "$INTERRUPT_AFTER_PROOF" = no ] || kill -TERM $$; }
            . "$1"
        ' shell "$case_root/customize.sh"
    actual=$?
    set -e
    case "$expected" in
        pass) [ "$actual" -eq 0 ] || fail "$name returned $actual, expected success" ;;
        reject) [ "$actual" -eq 73 ] || fail "$name returned $actual, expected installer refusal" ;;
        interrupted) [ "$actual" -ne 0 ] || fail "$name continued successfully after a caught signal" ;;
        *) fail "unknown expected result $expected" ;;
    esac
    printf '%s' "$expected_trace" >"$case_root/expected-trace"
    cmp -s "$case_root/expected-trace" "$case_root/trace" || {
        diff -u "$case_root/expected-trace" "$case_root/trace" >&2 || true
        fail "$name used the upgrade-proof lifecycle incorrectly"
    }
    if [ "$expected_frozen" = yes ]; then
        [ -e "$state/ready" ] && [ -e "$state/staged" ] || fail "$name resumed the old Runtime after a successful staged install"
        [ ! -e "$staging/disable" ] || fail "$name left an active paired upgrade disabled"
    else
        [ ! -e "$state/ready" ] && [ ! -e "$state/staged" ] || fail "$name left the old Runtime frozen after installer failure"
        [ -f "$staging/disable" ] || fail "$name changed the staged activation state on failure or fresh install"
    fi
}

run_case clean pass no no pass pass '' no 0 no no no
run_case retired_history pass retired no pass pass '' no 0 no no no
run_case live_prepare pass yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n' yes 1 no no no
run_case existing_proof pass yes yes pass pass $'--verify\n--stage\n' yes 1 no no no
run_case prepare_refused reject yes no freeze_fail pass $'--verify\n--prepare\n--cancel\n' no 1 no no no
run_case active_gate reject active no freeze_fail pass $'--verify\n--prepare\n--cancel\n' no 1 no no no
run_case stage_failed reject yes no pass reject $'--verify\n--prepare\n--verify\n--stage\n--cancel\n' no 1 no no no
run_case signal_after_freeze interrupted yes no pass pass $'--verify\n--prepare\n--verify\n--cancel\n' no 1 yes no no
run_case cancel_after_stage interrupted yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n--cancel\n' no 1 no yes no
run_case signal_during_commit pass yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n' yes 1 no no yes

ignore_line=$(grep -n "trap '' 0 HUP INT TERM" "$SOURCE" | tail -1 | cut -d: -f1)
restore_line=$(grep -n 'trap - 0 HUP INT TERM' "$SOURCE" | tail -1 | cut -d: -f1)
proof_clear_line=$(grep -n 'UPGRADE_PROOF_HELD=0' "$SOURCE" | tail -1 | cut -d: -f1)
staging_clear_line=$(grep -n 'UPGRADE_STAGING_ACTIVATED=0' "$SOURCE" | tail -1 | cut -d: -f1)
[ "$ignore_line" -lt "$proof_clear_line" ] && [ "$ignore_line" -lt "$staging_clear_line" ] &&
    [ "$restore_line" -gt "$proof_clear_line" ] && [ "$restore_line" -gt "$staging_clear_line" ] ||
    fail 'successful handoff ownership transition is outside the ignored-signal critical section'

printf '%s\n' 'KernelSU paired-upgrade gate scenarios passed.'
