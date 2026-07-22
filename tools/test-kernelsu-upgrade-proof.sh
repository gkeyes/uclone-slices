#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
SOURCE=$REPO_ROOT/slot-kernelsu/prepare-upgrade.sh
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/uclone-upgrade-proof.XXXXXX")
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM

fail() {
    printf 'KernelSU upgrade-proof test failed: %s\n' "$1" >&2
    exit 1
}

expect_failure() {
    if "$@" >/dev/null 2>&1; then
        fail "command unexpectedly succeeded: $*"
    fi
}

make_toybox() {
    target=$1
    cat >"$target" <<'EOF'
#!/bin/sh
command=$1
shift
case "$command" in
    stat)
        [ "${1:-}" = "-c" ] || exit 2
        format=$2
        path=$3
        mode=$(/usr/bin/stat -f '%Lp' "$path" 2>/dev/null || /usr/bin/stat -c '%a' "$path") || exit 2
        case "$format" in
            %u) printf '%s\n' 0 ;;
            %u:%g:%a) printf '0:0:%s\n' "$mode" ;;
            *) exit 2 ;;
        esac
        ;;
    id) [ "${1:-}" = "-u" ] && printf '%s\n' 0 ;;
    timeout)
        [ "${1:-}" = "-s" ] && shift 2
        shift
        exec "$@"
        ;;
    sha256sum) exec /usr/bin/shasum -a 256 "$@" ;;
    chown) exit 0 ;;
    *) exec "$command" "$@" ;;
esac
EOF
    chmod 0700 "$target"
}

make_slotctl() {
    target=$1
    slot=$2
    lifecycle=$3
    cat >"$target" <<EOF
#!/bin/sh
[ "\${1:-}" = apps ] || exit 2
printf '%s\\n' '{"schema_version":1,"status":"ok","payload":{"kind":"managed_apps","data":{"apps":[{"package":"com.asksky.fitness","active_slot":"$slot","lifecycle":"$lifecycle"}]}}}'
EOF
    chmod 0700 "$target"
}

RUNTIME=$SCRATCH/runtime
INSTALLED=$SCRATCH/installed
BOOT_ID=$SCRATCH/boot-id
UPTIME=$SCRATCH/uptime
TOYBOX=$SCRATCH/toybox
SCRIPT=$SCRATCH/prepare-upgrade.sh
mkdir -p "$INSTALLED/bin"
for root in enrollment compatibility-policy catalog registry package-state slot-metadata enrollment-attempts rescue-journal journal state; do
    mkdir -p "$RUNTIME/$root"
    chmod 0700 "$RUNTIME/$root"
done
mkdir -p "$RUNTIME/enrollment/packages/com.asksky.fitness"
printf '%s\n' enrolled >"$RUNTIME/enrollment/packages/com.asksky.fitness/enrollment.json"
chmod 0600 "$RUNTIME/enrollment/packages/com.asksky.fitness/enrollment.json"
printf '%s\n' test-boot >"$BOOT_ID"
printf '%s\n' '1000.00 20.00' >"$UPTIME"
make_toybox "$TOYBOX"
make_slotctl "$INSTALLED/bin/slotctl" base normal
sed \
    -e '1s|.*|#!/bin/sh|' \
    -e "s|^RUNTIME_ROOT=.*|RUNTIME_ROOT=$RUNTIME|" \
    -e "s|^INSTALLED_MODULE=.*|INSTALLED_MODULE=$INSTALLED|" \
    -e "s|^TOYBOX_BIN=.*|TOYBOX_BIN=$TOYBOX|" \
    -e "s|^BOOT_ID_FILE=.*|BOOT_ID_FILE=$BOOT_ID|" \
    -e "s|^UPTIME_FILE=.*|UPTIME_FILE=$UPTIME|" \
    "$SOURCE" >"$SCRIPT"
chmod 0700 "$SCRIPT"

mkdir -p "$RUNTIME/catalog/packages/com.asksky.fitness"
chmod 0755 "$RUNTIME/catalog/packages/com.asksky.fitness"
printf '%s\n' legacy >"$RUNTIME/catalog/packages/com.asksky.fitness/catalog.json"
chmod 0644 "$RUNTIME/catalog/packages/com.asksky.fitness/catalog.json"
"$SCRIPT" --prepare | grep -F 'upgrade-proof-ready apps=1' >/dev/null
[ "$("$SCRIPT" --verify)" = 1 ] || fail 'fresh proof did not verify'

chmod 0775 "$RUNTIME/catalog/packages/com.asksky.fitness"
expect_failure "$SCRIPT" --verify
chmod 0755 "$RUNTIME/catalog/packages/com.asksky.fitness"
[ "$("$SCRIPT" --verify)" = 1 ] || fail 'root-owned legacy metadata did not recover after unsafe mode reset'

printf '%s\n' drift >"$RUNTIME/registry/drift.json"
chmod 0600 "$RUNTIME/registry/drift.json"
expect_failure "$SCRIPT" --verify
rm "$RUNTIME/registry/drift.json"
[ "$("$SCRIPT" --verify)" = 1 ] || fail 'restored digest did not verify'

printf '%s\n' another-boot >"$BOOT_ID"
expect_failure "$SCRIPT" --verify
printf '%s\n' test-boot >"$BOOT_ID"
[ "$("$SCRIPT" --verify)" = 1 ] || fail 'same-boot proof did not recover after fixture reset'

printf '%s\n' '2000.00 20.00' >"$UPTIME"
expect_failure "$SCRIPT" --verify
printf '%s\n' '1001.00 20.00' >"$UPTIME"
[ "$("$SCRIPT" --verify)" = 1 ] || fail 'proof inside freshness window failed'

: >"$RUNTIME/state/com.asksky.fitness.gate"
chmod 0600 "$RUNTIME/state/com.asksky.fitness.gate"
expect_failure "$SCRIPT" --verify
rm "$RUNTIME/state/com.asksky.fitness.gate"

"$SCRIPT" --consume
expect_failure "$SCRIPT" --verify

make_slotctl "$INSTALLED/bin/slotctl" preview normal
expect_failure "$SCRIPT" --prepare

printf '%s\n' 'KernelSU one-time upgrade proof scenarios passed.'
