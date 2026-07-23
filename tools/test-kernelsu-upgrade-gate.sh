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
    pidof)
        [ "${1:-}" = ucloned ] || exit 4
        case "${OLD_DAEMON_RESULT:-absent}" in
            absent) exit 1 ;;
            trusted) printf '%s\n' 123 ;;
            error) exit 2 ;;
            *) exit 3 ;;
        esac
        ;;
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
    installed_state=${13:-enabled}
    old_daemon_result=${14:-trusted}
    expected_ui=${15:-}
    case_root=$SCRATCH/$name
    runtime=$case_root/runtime
    installed=$case_root/installed
    staging=$case_root/staging
    state=$case_root/upgrade-state
    mkdir -p "$runtime" "$staging" "$state"
    make_toybox "$case_root/toybox"
    make_upgrade_tool "$staging/prepare-upgrade.sh"
    printf '%s\n' '#!/bin/sh' >"$staging/upgrade-freeze.sh"
    chmod 0700 "$staging/upgrade-freeze.sh"
    : >"$staging/disable"
    chmod 0644 "$staging/disable"
    case "$installed_state" in
        enabled) mkdir -p "$installed/bin" ;;
        disabled)
            mkdir -p "$installed/bin"
            : >"$installed/disable"
            chmod 0644 "$installed/disable"
            ;;
        missing) ;;
        *) fail "unknown installed module state $installed_state" ;;
    esac
    case "$metadata" in
        yes) mkdir -p "$runtime/enrollment/packages/com.asksky.fitness"; : >"$runtime/enrollment/packages/com.asksky.fitness/enrollment.json" ;;
        active) mkdir -p "$runtime/state"; : >"$runtime/state/com.asksky.fitness.gate" ;;
        retired) mkdir -p "$runtime/state"; : >"$runtime/state/.com.asksky.fitness.gate.retired" ;;
        retired_managed)
            mkdir -p "$runtime/enrollment/packages/com.asksky.fitness" "$runtime/state"
            : >"$runtime/enrollment/packages/com.asksky.fitness/enrollment.json"
            : >"$runtime/state/.com.asksky.fitness.gate.retired"
            ;;
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
    : >"$case_root/ui"
    set +e
    UPGRADE_TRACE=$case_root/trace \
        UI_TRACE=$case_root/ui \
        UPGRADE_STATE=$state \
        PREPARE_RESULT=$prepare_result \
        CONSUME_RESULT=$consume_result \
        PROOF_COUNT=$proof_count \
        OLD_DAEMON_RESULT=$old_daemon_result \
        INTERRUPT_AFTER_PROOF=$interrupt \
        INTERRUPT_AFTER_ACTIVATION=$activation_interrupt \
        STAGED_DISABLE=$staging/disable \
        MODPATH=$staging /bin/sh -c '
            ui_print() { printf "%s\n" "$*" >>"$UI_TRACE"; }
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
    case "$expected_frozen" in
        yes)
            [ -e "$state/ready" ] && [ -e "$state/staged" ] ||
                fail "$name resumed the old Runtime after a successful staged install"
            [ ! -e "$staging/disable" ] ||
                fail "$name left an active paired upgrade disabled"
            ;;
        recovery)
            [ ! -e "$state/ready" ] && [ ! -e "$state/staged" ] ||
                fail "$name created a paired-upgrade proof during offline recovery"
            [ ! -e "$staging/disable" ] ||
                fail "$name did not schedule the recovery Runtime for next boot"
            ;;
        no)
            [ ! -e "$state/ready" ] && [ ! -e "$state/staged" ] ||
                fail "$name left the old Runtime frozen after installer failure"
            [ -f "$staging/disable" ] ||
                fail "$name changed the staged activation state on failure or fresh install"
            ;;
        *) fail "unknown expected frozen state $expected_frozen" ;;
    esac
    if [ -n "$expected_ui" ]; then
        grep -F "$expected_ui" "$case_root/ui" >/dev/null ||
            fail "$name did not explain the fail-closed recovery reinstall"
        case "$metadata" in
            active) [ -f "$runtime/state/com.asksky.fitness.gate" ] ||
                fail "$name changed the active Gate evidence" ;;
            retired_managed)
                [ -f "$runtime/enrollment/packages/com.asksky.fitness/enrollment.json" ] &&
                    [ -f "$runtime/state/.com.asksky.fitness.gate.retired" ] ||
                    fail "$name changed existing retired recovery state"
                ;;
        esac
        case "$installed_state" in
            enabled) [ -d "$installed" ] && [ ! -e "$installed/disable" ] ||
                fail "$name changed the active installed-module marker state" ;;
            disabled) [ -f "$installed/disable" ] ||
                fail "$name changed the installed disable marker" ;;
            missing) [ ! -e "$installed" ] && [ ! -L "$installed" ] ||
                fail "$name created or replaced the missing installed module" ;;
        esac
    fi
}

run_case clean pass no no pass pass '' no 0 no no no
run_case retired_history pass retired no pass pass '' no 0 no no no
run_case live_prepare pass yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n' yes 1 no no no
run_case existing_proof pass yes yes pass pass $'--verify\n--stage\n' yes 1 no no no
run_case prepare_refused reject yes no freeze_fail pass $'--verify\n--prepare\n--cancel\n' no 1 no no no
run_case active_gate_live_daemon reject active no freeze_fail pass \
    $'--verify\n--prepare\n--cancel\n' no 1 no no no enabled trusted
run_case stage_failed reject yes no pass reject $'--verify\n--prepare\n--verify\n--stage\n--cancel\n' no 1 no no no
run_case signal_after_freeze interrupted yes no pass pass $'--verify\n--prepare\n--verify\n--cancel\n' no 1 yes no no
run_case cancel_after_stage interrupted yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n--cancel\n' no 1 no yes no
run_case signal_during_commit pass yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n' yes 1 no no yes
run_case orphaned_disabled_recovery pass retired_managed no freeze_fail pass '' recovery 1 no no no \
    disabled absent \
    'Recovery reinstall activates on next boot and does not prove Base, retired, or complete.'
run_case active_gate_disabled_recovery pass active no freeze_fail pass '' recovery 1 no no no \
    disabled absent \
    'Recovery reinstall activates on next boot and does not prove Base, retired, or complete.'
run_case active_gate_ksu_removed_disable_recovery pass active no freeze_fail pass '' \
    recovery 1 no no no enabled absent \
    'Recovery reinstall activates on next boot and does not prove Base, retired, or complete.'
run_case disabled_live_daemon pass yes no pass pass $'--verify\n--prepare\n--verify\n--stage\n' \
    yes 1 no no no disabled trusted
run_case disabled_daemon_probe_error reject yes no freeze_fail pass \
    $'--verify\n--prepare\n--cancel\n' no 1 no no no disabled error
run_case missing_module_recovery pass retired_managed no freeze_fail pass '' \
    recovery 1 no no no missing absent \
    'Recovery reinstall activates on next boot and does not prove Base, retired, or complete.'
run_case missing_module_live_daemon reject yes no freeze_fail pass \
    $'--verify\n--prepare\n--cancel\n' no 1 no no no missing trusted

ignore_line=$(grep -n "trap '' 0 HUP INT TERM" "$SOURCE" | tail -1 | cut -d: -f1)
restore_line=$(awk -v start="$ignore_line" 'NR > start && /trap - 0 HUP INT TERM/ { print NR; exit }' "$SOURCE")
proof_clear_line=$(awk -v start="$ignore_line" 'NR > start && /UPGRADE_PROOF_HELD=0/ { print NR; exit }' "$SOURCE")
staging_clear_line=$(awk -v start="$ignore_line" 'NR > start && /UPGRADE_STAGING_ACTIVATED=0/ { print NR; exit }' "$SOURCE")
[ "$ignore_line" -lt "$proof_clear_line" ] && [ "$ignore_line" -lt "$staging_clear_line" ] &&
    [ "$restore_line" -gt "$proof_clear_line" ] && [ "$restore_line" -gt "$staging_clear_line" ] ||
    fail 'successful handoff ownership transition is outside the ignored-signal critical section'

printf '%s\n' 'KernelSU paired-upgrade gate scenarios passed.'
