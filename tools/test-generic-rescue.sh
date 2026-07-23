#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-generic-rescue.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
mkdir -p "$FIXTURE/bin" "$FIXTURE/runtime/logs" "$FIXTURE/profile"
"$ROOT/tools/render-target-profile.sh" generic "$FIXTURE/profile" >/dev/null
sed "s|^readonly UCLONE_RUNTIME_ROOT=.*$|readonly UCLONE_RUNTIME_ROOT='$FIXTURE/runtime'|" \
    "$FIXTURE/profile/target-profile-generic/target-profile.sh" \
    >"$FIXTURE/target-profile.sh"
cp "$ROOT/slot-kernelsu/rescue.sh" "$FIXTURE/rescue.sh"

cat >"$FIXTURE/bin/toybox" <<'EOF'
#!/bin/sh
case "${1:-}" in
    stat)
        [ "${2:-}" = -L ] && shift
        if [ "${2:-}:${3:-}" = "-c:%d:%i" ]; then
            case "${4:-}" in
                /proc/1/ns/mnt) printf '%s\n' '253:801'; exit 0 ;;
                /proc/self/ns/mnt)
                    [ "${FAKE_IN_PID1_NS:-}" = 1 ] || exit 1
                    [ "${NAMESPACE_MISMATCH:-}" = 1 ] && printf '%s\n' '253:802' || printf '%s\n' '253:801'
                    exit 0
                    ;;
                *) printf '%s\n' '253:900' ;;
            esac
            exit 0
        fi
        case "${2:-}:${3:-}" in
            -c:%u:%g) printf '%s\n' '0:0' ;;
            -c:%a) printf '%s\n' '755' ;;
            *) exit 1 ;;
        esac
        ;;
    nsenter)
        while [ "$#" -gt 0 ] && [ "$1" != -- ]; do shift; done
        shift
        FAKE_IN_PID1_NS=1 exec "$@"
        ;;
    sha256sum)
        shift
        exec /usr/bin/shasum -a 256 "$@"
        ;;
    grep)
        shift
        exec /usr/bin/grep "$@"
        ;;
    *) exit 1 ;;
esac
EOF
cat >"$FIXTURE/bin/slotctl" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >>'$FIXTURE/slotctl.log'
exit 0
EOF
chmod 755 "$FIXTURE/rescue.sh" "$FIXTURE/bin/toybox" "$FIXTURE/bin/slotctl"
sed -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$FIXTURE/bin/toybox'|" \
    -e 's|^SYSTEM_SHELL=.*$|SYSTEM_SHELL=/bin/sh|' \
    -e 's|^FD_ROOT=.*$|FD_ROOT=/dev/fd|' \
    -e 's|^FD_EXEC_INTERPRETER=.*$|FD_EXEC_INTERPRETER=/bin/sh|' \
    -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$FIXTURE/runtime'|" \
    "$FIXTURE/rescue.sh" >"$FIXTURE/rescue.patched"
mv "$FIXTURE/rescue.patched" "$FIXTURE/rescue.sh"
chmod 755 "$FIXTURE/rescue.sh"

/bin/sh "$FIXTURE/rescue.sh" com.asksky.fitness --to-base >/dev/null
grep -F -x 'rescue com.asksky.fitness --to-base' "$FIXTURE/slotctl.log" >/dev/null
before="$(wc -l <"$FIXTURE/slotctl.log" | tr -d ' ')"
if /bin/sh "$FIXTURE/rescue.sh" '../../bad' --to-base >/dev/null 2>&1; then
    printf '%s\n' 'generic rescue accepted an invalid package' >&2
    exit 1
fi
after="$(wc -l <"$FIXTURE/slotctl.log" | tr -d ' ')"
[ "$before" = "$after" ] || {
    printf '%s\n' 'invalid package reached slotctl' >&2
    exit 1
}
if NAMESPACE_MISMATCH=1 /bin/sh "$FIXTURE/rescue.sh" com.asksky.fitness --to-base >/dev/null 2>&1; then
    printf '%s\n' 'generic rescue crossed a mismatched mount namespace' >&2
    exit 1
fi
[ "$(wc -l <"$FIXTURE/slotctl.log" | tr -d ' ')" = "$after" ] || {
    printf '%s\n' 'namespace mismatch reached slotctl' >&2
    exit 1
}
printf '%s\n' 'Generic offline rescue validates package and rejects PID1 mount namespace mismatch.'
