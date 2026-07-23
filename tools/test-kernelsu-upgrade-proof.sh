#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
PROOF_SOURCE=$REPO_ROOT/slot-kernelsu/prepare-upgrade.sh
FREEZE_SOURCE=$REPO_ROOT/slot-kernelsu/upgrade-freeze.sh
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/uclone-upgrade-proof.XXXXXX")
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM
DAEMON_PID=7654321

fail() {
    printf 'KernelSU upgrade-proof test failed: %s\n' "$1" >&2
    exit 1
}

expect_failure() {
    if "$@" >/dev/null 2>&1; then
        fail "command unexpectedly succeeded: $*"
    fi
}

write_process_state() {
    state=$1
    printf '%s (ucloned) %s 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 777 0 0 0\n' \
        "$DAEMON_PID" "$state" >"$PROC/$DAEMON_PID/stat"
}

make_toybox() {
    target=$1
    cat >"$target" <<'EOF'
#!/bin/sh
command=$1
shift
case "$command" in
    stat)
        [ "${1:-}" = "-L" ] && shift
        [ "${1:-}" = "-c" ] || exit 2
        format=$2
        path=$3
        if [ "$format" = '%d:%i' ]; then
            case "$path" in
                /proc/1/ns/mnt) printf '%s\n' '253:701'; exit 0 ;;
                /proc/self/ns/mnt)
                    [ "${FAKE_IN_PID1_NS:-}" = 1 ] || exit 1
                    if [ "${NAMESPACE_MISMATCH:-}" = 1 ]; then
                        printf '%s\n' '253:702'
                    else
                        printf '%s\n' '253:701'
                    fi
                    exit 0
                    ;;
                /dev/fd/9)
                    if [ "${MISMATCH_SLOTCTL_NODE:-}" = 1 ]; then
                        printf '%s\n' '253:1000'
                        exit 0
                    fi
                    ;;
            esac
        fi
        mode=$(/usr/bin/stat -c '%a' "$path" 2>/dev/null || /usr/bin/stat -f '%Lp' "$path") || exit 2
        case "$format" in
            %u) printf '%s\n' 0 ;;
            %u:%g:%a) printf '0:0:%s\n' "$mode" ;;
            %d:%i) printf '%s\n' "${FAKE_IMAGE_ID:-253:999}" ;;
            *) exit 2 ;;
        esac
        ;;
    id) [ "${1:-}" = "-u" ] && printf '%s\n' 0 ;;
    timeout)
        [ "${1:-}" = "-s" ] && shift 2
        shift
        exec "$@"
        ;;
    nsenter)
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --) shift; break ;;
                -t) shift 2 ;;
                -m) shift ;;
                *) exit 2 ;;
            esac
        done
        FAKE_IN_PID1_NS=1 exec "$@"
        ;;
    pidof)
        [ -f "$FAKE_PID_FILE" ] && cat "$FAKE_PID_FILE"
        ;;
    kill)
        signal=$1
        pid=$2
        [ "$pid" = "$FAKE_DAEMON_PID" ] || exit 1
        case "$signal" in
            -STOP)
                sed 's/) [A-Z] /) T /' "$FAKE_STAT_FILE" >"$FAKE_STAT_FILE.new"
                mv "$FAKE_STAT_FILE.new" "$FAKE_STAT_FILE"
                [ -z "${DRIFT_ON_STOP:-}" ] || printf 'drift\n' >"$DRIFT_ON_STOP"
                [ -z "${GATE_ON_STOP:-}" ] || : >"$GATE_ON_STOP"
                ;;
            -CONT)
                [ -z "${REQUIRE_DISABLE_MARKER:-}" ] || [ -f "$REQUIRE_DISABLE_MARKER" ] || exit 1
                sed 's/) T /) S /' "$FAKE_STAT_FILE" >"$FAKE_STAT_FILE.new"
                mv "$FAKE_STAT_FILE.new" "$FAKE_STAT_FILE"
                ;;
            *) exit 2 ;;
        esac
        printf '%s %s\n' "$signal" "$pid" >>"$SIGNAL_TRACE"
        ;;
    sleep) : ;;
    sha256sum)
        if [ -n "${MISMATCH_PROCESS_DIGEST:-}" ] && [ "${1:-}" = "$FAKE_PROCESS_EXE" ]; then
            printf '%064d  %s\n' 0 "$1"
        elif [ "${MISMATCH_SLOTCTL_DIGEST:-}" = 1 ] && [ "${1:-}" = /dev/fd/9 ]; then
            printf '%064d  %s\n' 0 "$1"
        else
            exec /usr/bin/shasum -a 256 "$@"
        fi
        ;;
    chown) exit 0 ;;
    *) exec "$command" "$@" ;;
esac
EOF
    chmod 0700 "$target"
}

make_slotctl() {
    slot=$1
    lifecycle=$2
    cat >"$SCRATCH/bin/slotctl" <<EOF
#!/bin/sh
[ "\${1:-}" = upgrade-readiness ] && [ "\$#" -eq 1 ] || exit 2
printf '%s\n' upgrade-readiness >>"\$SLOTCTL_TRACE"
[ "$slot" = base ] && [ "$lifecycle" = normal ] || exit 1
printf '%s\n' 1
EOF
    chmod 0700 "$SCRATCH/bin/slotctl"
}

make_empty_slotctl() {
    cat >"$SCRATCH/bin/slotctl" <<'EOF'
#!/bin/sh
[ "${1:-}" = upgrade-readiness ] && [ "$#" -eq 1 ] || exit 2
printf '%s\n' upgrade-readiness >>"$SLOTCTL_TRACE"
printf '%s\n' 0
EOF
    chmod 0700 "$SCRATCH/bin/slotctl"
}

make_simplified_slotctl() {
    cat >"$SCRATCH/bin/slotctl" <<'EOF'
#!/bin/sh
[ "${1:-}" = upgrade-readiness ] && [ "$#" -eq 1 ] || exit 2
printf '%s\n' '0'
printf '%s\n' 'unexpected'
EOF
    chmod 0700 "$SCRATCH/bin/slotctl"
}

RUNTIME=$SCRATCH/runtime
INSTALLED=$SCRATCH/installed
PROC=$SCRATCH/proc
BOOT_ID=$SCRATCH/boot-id
UPTIME=$SCRATCH/uptime
TOYBOX=$SCRATCH/toybox
PROOF=$SCRATCH/prepare-upgrade.sh
FREEZE=$SCRATCH/upgrade-freeze.sh
mkdir -p "$INSTALLED/bin" "$SCRATCH/bin" "$PROC/$DAEMON_PID"
for root in enrollment compatibility-policy catalog registry package-state slot-metadata enrollment-attempts rescue-journal journal state; do
    mkdir -p "$RUNTIME/$root"
    chmod 0700 "$RUNTIME/$root"
done
mkdir -p "$RUNTIME/enrollment/packages/com.asksky.fitness"
printf '%s\n' enrolled >"$RUNTIME/enrollment/packages/com.asksky.fitness/enrollment.json"
chmod 0600 "$RUNTIME/enrollment/packages/com.asksky.fitness/enrollment.json"
printf '%s\n' test-boot >"$BOOT_ID"
printf '%s\n' '1000.00 20.00' >"$UPTIME"
printf '%s\n' "$DAEMON_PID" >"$SCRATCH/pids"
: >"$SCRATCH/signals"
: >"$SCRATCH/slotctl-trace"
printf '%s\n' '#!/bin/sh' >"$INSTALLED/bin/ucloned"
chmod 0700 "$INSTALLED/bin/ucloned"
ln -s "$INSTALLED/bin/ucloned" "$PROC/$DAEMON_PID/exe"
write_process_state S
make_toybox "$TOYBOX"
make_slotctl base normal

sed \
    -e '1s|.*|#!/bin/sh|' \
    -e "s|^INSTALLED_MODULE=.*|INSTALLED_MODULE=$INSTALLED|" \
    -e "s|^TOYBOX_BIN=.*|TOYBOX_BIN=$TOYBOX|" \
    -e 's|^SYSTEM_SHELL=.*|SYSTEM_SHELL=/bin/sh|' \
    -e "s|^PROC_ROOT=.*|PROC_ROOT=$PROC|" \
    "$FREEZE_SOURCE" >"$FREEZE"
chmod 0700 "$FREEZE"
sed \
    -e '1s|.*|#!/bin/sh|' \
    -e "s|^RUNTIME_ROOT=.*|RUNTIME_ROOT=$RUNTIME|" \
    -e "s|^INSTALLED_MODULE=.*|INSTALLED_MODULE=$INSTALLED|" \
    -e "s|^TOYBOX_BIN=.*|TOYBOX_BIN=$TOYBOX|" \
    -e 's|^SYSTEM_SHELL=.*|SYSTEM_SHELL=/bin/sh|' \
    -e 's|^FD_ROOT=.*|FD_ROOT=/dev/fd|' \
    -e 's|^FD_EXEC_INTERPRETER=.*|FD_EXEC_INTERPRETER=/bin/sh|' \
    -e "s|^FREEZE_HELPER=.*|FREEZE_HELPER=$FREEZE|" \
    -e "s|^BOOT_ID_FILE=.*|BOOT_ID_FILE=$BOOT_ID|" \
    -e "s|^UPTIME_FILE=.*|UPTIME_FILE=$UPTIME|" \
    "$PROOF_SOURCE" >"$PROOF"
chmod 0700 "$PROOF"

export FAKE_PID_FILE=$SCRATCH/pids
export FAKE_DAEMON_PID=$DAEMON_PID
export FAKE_STAT_FILE=$PROC/$DAEMON_PID/stat
export SIGNAL_TRACE=$SCRATCH/signals
export FAKE_IMAGE_ID=253:999
export FAKE_PROCESS_EXE=$PROC/$DAEMON_PID/exe
export SLOTCTL_TRACE=$SCRATCH/slotctl-trace

"$PROOF" --prepare | grep -F 'upgrade-proof-ready apps=1' >/dev/null
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = T ] || fail 'prepare did not freeze the old daemon'
[ "$("$PROOF" --verify)" = 1 ] || fail 'fresh frozen proof did not verify'
grep -Fx -- "-STOP $DAEMON_PID" "$SCRATCH/signals" >/dev/null || fail 'SIGSTOP was not issued'

printf '%s\n' drift >"$RUNTIME/registry/drift.json"
chmod 0600 "$RUNTIME/registry/drift.json"
expect_failure "$PROOF" --verify
"$PROOF" --cancel
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'cancel did not resume the old daemon after metadata drift'
expect_failure "$PROOF" --verify
rm "$RUNTIME/registry/drift.json"

"$PROOF" --prepare >/dev/null
"$PROOF" --stage
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = T ] || fail 'staged install did not keep the old daemon frozen'
expect_failure "$PROOF" --verify
printf '%s\n' another-boot >"$BOOT_ID"
expect_failure "$PROOF" --cancel
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = T ] || fail 'cross-boot cancellation resumed an unproven process'
printf '%s\n' test-boot >"$BOOT_ID"
export REQUIRE_DISABLE_MARKER=$SCRATCH/disable
"$PROOF" --cancel
unset REQUIRE_DISABLE_MARKER
[ -f "$SCRATCH/disable" ] || fail 'cancel after stage left the staged module enabled'
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'explicit cancellation did not resume the old daemon'

"$PROOF" --prepare >/dev/null
sed 's/ 777 / 778 /' "$PROC/$DAEMON_PID/stat" >"$PROC/$DAEMON_PID/stat.new"
mv "$PROC/$DAEMON_PID/stat.new" "$PROC/$DAEMON_PID/stat"
expect_failure "$PROOF" --cancel
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = T ] || fail 'identity mismatch resumed an unproven process'
sed 's/ 778 / 777 /' "$PROC/$DAEMON_PID/stat" >"$PROC/$DAEMON_PID/stat.new"
mv "$PROC/$DAEMON_PID/stat.new" "$PROC/$DAEMON_PID/stat"
"$PROOF" --cancel

export DRIFT_ON_STOP=$RUNTIME/catalog/drift-during-stop
expect_failure "$PROOF" --prepare
unset DRIFT_ON_STOP
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'failed prepare left the daemon stopped'
[ ! -e "$RUNTIME/upgrade/base-proof" ] || fail 'failed prepare left a published proof'
rm "$RUNTIME/catalog/drift-during-stop"

export GATE_ON_STOP=$RUNTIME/state/com.asksky.fitness.gate
expect_failure "$PROOF" --prepare
unset GATE_ON_STOP
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'gate race left the daemon stopped'
rm "$RUNTIME/state/com.asksky.fitness.gate"

make_slotctl base normal
"$PROOF" --prepare >/dev/null
sed 's/^2|ready|/2|preparing|/' "$RUNTIME/upgrade/base-proof" >"$RUNTIME/upgrade/base-proof.new"
mv "$RUNTIME/upgrade/base-proof.new" "$RUNTIME/upgrade/base-proof"
chmod 0600 "$RUNTIME/upgrade/base-proof"
"$PROOF" --cancel
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'interrupted preparing proof could not resume the exact daemon'

make_slotctl preview normal
expect_failure "$PROOF" --prepare
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'non-Base refusal stopped the daemon'

make_slotctl base recovery_required
expect_failure "$PROOF" --prepare
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'non-normal refusal stopped the daemon'

make_empty_slotctl
"$PROOF" --prepare | grep -F 'upgrade-proof-ready apps=0' >/dev/null
[ "$($PROOF --verify)" = 0 ] || fail 'canonical empty managed-app report did not verify as count zero'
"$PROOF" --cancel
make_simplified_slotctl
expect_failure "$PROOF" --prepare

make_slotctl base normal
export MISMATCH_PROCESS_DIGEST=1
expect_failure "$PROOF" --prepare
unset MISMATCH_PROCESS_DIGEST
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'untrusted daemon image was frozen'

printf '%s %s\n' "$DAEMON_PID" "$((DAEMON_PID + 1))" >"$SCRATCH/pids"
expect_failure "$PROOF" --prepare
printf '%s\n' "$DAEMON_PID" >"$SCRATCH/pids"

slotctl_before=$(wc -l <"$SCRATCH/slotctl-trace" | tr -d '[:space:]')
export NAMESPACE_MISMATCH=1
expect_failure "$PROOF" --prepare
expect_failure "$FREEZE" --inspect
unset NAMESPACE_MISMATCH
[ "$(wc -l <"$SCRATCH/slotctl-trace" | tr -d '[:space:]')" = "$slotctl_before" ] ||
    fail 'namespace mismatch executed staged slotctl'
[ "$(awk '{print $3}' "$PROC/$DAEMON_PID/stat")" = S ] || fail 'namespace mismatch reached or froze the old daemon'

export MISMATCH_SLOTCTL_NODE=1
expect_failure "$PROOF" --prepare
unset MISMATCH_SLOTCTL_NODE
export MISMATCH_SLOTCTL_DIGEST=1
expect_failure "$PROOF" --prepare
unset MISMATCH_SLOTCTL_DIGEST
[ "$(wc -l <"$SCRATCH/slotctl-trace" | tr -d '[:space:]')" = "$slotctl_before" ] ||
    fail 'slotctl inode or digest mismatch reached execution'

printf '%s\n' 'KernelSU frozen one-time upgrade proof scenarios passed.'
