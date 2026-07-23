#!/usr/bin/env bash

set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/uclone-post-fs-setup.XXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT HUP INT TERM
BIN="$FIXTURE/bin"
BOOT_ID_FILE="$FIXTURE/boot-id"

host_mode() {
    /usr/bin/stat -c '%a' "$1" 2>/dev/null || /usr/bin/stat -f '%Lp' "$1"
}

mkdir -p "$BIN"
printf '%s\n' boot-current >"$BOOT_ID_FILE"
cat >"$BIN/toybox" <<'EOF'
#!/bin/sh
case "${1:-}" in
    stat)
        format=${3:-}
        path=${4:-}
        case "$format" in
            %u:%g) printf '%s\n' '0:0' ;;
            %u:%g:%a)
                mode=$(/usr/bin/stat -c '%a' "$path" 2>/dev/null) ||
                    mode=$(/usr/bin/stat -f '%Lp' "$path")
                printf '0:0:%s\n' "$mode"
                ;;
            *) exit 1 ;;
        esac
        ;;
    chown) exit 0 ;;
    mkdir) shift; exec /bin/mkdir "$@" ;;
    chmod) shift; exec /bin/chmod "$@" ;;
    mv) shift; exec /bin/mv "$@" ;;
    cat) shift; exec /bin/cat "$@" ;;
    rm) shift; exec /bin/rm "$@" ;;
    *) exit 1 ;;
esac
EOF
chmod 755 "$BIN/toybox"

prepare_script() {
    runtime="$1"
    output="$2"
    sed -e '1s|.*|#!/bin/sh|' \
        -e "s|^UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview$|UCLONE_RUNTIME_ROOT='$runtime'|" \
        -e "s|^TOYBOX_BIN=.*$|TOYBOX_BIN='$BIN/toybox'|" \
        -e "s|^BOOT_ID_FILE=.*$|BOOT_ID_FILE='$BOOT_ID_FILE'|" \
        "$ROOT/slot-kernelsu/post-fs-setup.sh" >"$output"
    chmod 755 "$output"
}

MIGRATED="$FIXTURE/migrated"
mkdir -p "$MIGRATED/run"
printf '%s\n' 1 >"$MIGRATED/version"
printf '%s\n' boot-old >"$MIGRATED/run/startup-gate.ready"
chmod 600 "$MIGRATED/version" "$MIGRATED/run/startup-gate.ready"
prepare_script "$MIGRATED" "$FIXTURE/migrate.sh"
/bin/sh "$FIXTURE/migrate.sh"
[ "$(cat "$MIGRATED/version")" = 2 ]
[ "$(cat "$MIGRATED/run/startup-gate.pending")" = boot-current ]
[ ! -e "$MIGRATED/run/startup-gate.ready" ]
[ "$(host_mode "$MIGRATED/version")" = 600 ]

STALE="$FIXTURE/stale"
mkdir -p "$STALE/run"
printf '%s\n' 1 >"$STALE/version"
printf '%s\n' 2 >"$STALE/.version.new"
printf '%s\n' boot-old >"$STALE/run/startup-gate.ready"
chmod 600 "$STALE/version" "$STALE/.version.new" "$STALE/run/startup-gate.ready"
prepare_script "$STALE" "$FIXTURE/stale.sh"
/bin/sh "$FIXTURE/stale.sh"
[ "$(cat "$STALE/version")" = 2 ]
[ ! -e "$STALE/.version.new" ]

UNSAFE="$FIXTURE/unsafe"
mkdir -p "$UNSAFE/run"
printf '%s\n' 1 >"$UNSAFE/version"
printf '%s\n' 2 >"$UNSAFE/not-version"
ln -s "$UNSAFE/not-version" "$UNSAFE/.version.new"
printf '%s\n' boot-old >"$UNSAFE/run/startup-gate.ready"
chmod 600 "$UNSAFE/version" "$UNSAFE/not-version" "$UNSAFE/run/startup-gate.ready"
prepare_script "$UNSAFE" "$FIXTURE/unsafe.sh"
if /bin/sh "$FIXTURE/unsafe.sh"; then
    printf '%s\n' 'unsafe version temporary was accepted' >&2
    exit 1
fi
[ "$(cat "$UNSAFE/version")" = 1 ]
[ "$(cat "$UNSAFE/run/startup-gate.ready")" = boot-old ]

REJECTED="$FIXTURE/rejected"
mkdir -p "$REJECTED/run"
printf '%s\n' 3 >"$REJECTED/version"
printf '%s\n' boot-old >"$REJECTED/run/startup-gate.ready"
chmod 600 "$REJECTED/version" "$REJECTED/run/startup-gate.ready"
prepare_script "$REJECTED" "$FIXTURE/reject.sh"
if /bin/sh "$FIXTURE/reject.sh"; then
    printf '%s\n' 'unsupported Runtime version was migrated' >&2
    exit 1
fi
[ "$(cat "$REJECTED/version")" = 3 ]
[ "$(cat "$REJECTED/run/startup-gate.ready")" = boot-old ]
printf '%s\n' 'Full post-fs setup migration and rejection paths passed.'
