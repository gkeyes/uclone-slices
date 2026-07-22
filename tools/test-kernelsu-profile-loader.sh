#!/usr/bin/env bash
set -euo pipefail

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/uclone-profile-loader.XXXXXX")
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM
TOYBOX=$SCRATCH/toybox
LOADER=$SCRATCH/profile-loader.sh
PROFILE=$SCRATCH/target-profile.sh

cat >"$TOYBOX" <<'EOF'
#!/bin/sh
case "$1" in
    stat) printf '%s\n' "${PROFILE_OWNER_MODE:-0:444}" ;;
    sed) shift; exec /usr/bin/sed "$@" ;;
    *) exit 1 ;;
esac
EOF
chmod 755 "$TOYBOX"
sed "s|/system/bin/toybox|$TOYBOX|g" "$ROOT/slot-kernelsu/profile-loader.sh" >"$LOADER"
cat >"$PROFILE" <<'EOF'
readonly UCLONE_TARGET_PROFILE='fitness'
readonly UCLONE_TARGET_PACKAGE='com.asksky.fitness'
readonly UCLONE_TARGET_USER='0'
EOF
chmod 444 "$PROFILE"

result=$(PROFILE_OWNER_MODE=0:444 /bin/sh -c ". '$LOADER'; uclone_load_profile '$PROFILE'; printf '%s:%s:%s' \"\$UCLONE_TARGET_PROFILE\" \"\$UCLONE_TARGET_PACKAGE\" \"\$UCLONE_TARGET_USER\"")
[ "$result" = fitness:com.asksky.fitness:0 ]

result=$(PROFILE_OWNER_MODE=0:644 /bin/sh -c ". '$LOADER'; uclone_load_profile '$PROFILE' || :; printf '%s:%s' \"\$UCLONE_TARGET_PROFILE\" \"\$UCLONE_TARGET_PACKAGE\"")
[ "$result" = generic:com.uclone.slots.preview ]

ln -s "$PROFILE" "$SCRATCH/profile-link.sh"
result=$(PROFILE_OWNER_MODE=0:444 /bin/sh -c ". '$LOADER'; uclone_load_profile '$SCRATCH/profile-link.sh' || :; printf '%s:%s' \"\$UCLONE_TARGET_PROFILE\" \"\$UCLONE_TARGET_PACKAGE\"")
[ "$result" = generic:com.uclone.slots.preview ]

result=$(PROFILE_OWNER_MODE=0:444 /bin/sh -c ". '$LOADER'; uclone_load_profile '$SCRATCH/missing.sh' || :; printf '%s:%s' \"\$UCLONE_TARGET_PROFILE\" \"\$UCLONE_TARGET_PACKAGE\"")
[ "$result" = generic:com.uclone.slots.preview ]
printf '%s\n' 'KernelSU profile loader rejects missing, mutable, and symlink profiles.'
