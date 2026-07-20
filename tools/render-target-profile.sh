#!/usr/bin/env bash

set -euo pipefail
umask 077

fail() {
    printf 'target profile error: %s\n' "$1" >&2
    exit 1
}

if (( $# != 2 )); then
    fail 'usage is render-target-profile.sh <slotprobe|fitness|generic> <existing-output-root>'
fi

profile="$1"
case "$profile" in
    slotprobe) expected_package=com.uclone.slotprobe ;;
    fitness) expected_package=com.asksky.fitness ;;
    generic) expected_package=com.uclone.slots.preview ;;
    *) fail 'profile must be exactly slotprobe, fitness, or generic' ;;
esac

output_input="$2"
[ -d "$output_input" ] || fail 'output root must be an existing directory'
[ ! -L "$output_input" ] || fail 'output root must not be a symlink'
output_root="$(CDPATH= cd -- "$output_input" && pwd -P)"

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
root_dir="$(CDPATH= cd -- "$script_dir/.." && pwd -P)"
profile_file="$root_dir/slot-targets/$profile.toml"
[ -f "$profile_file" ] || fail 'selected profile is not a regular file'
[ ! -L "$profile_file" ] || fail 'selected profile must not be a symlink'

line_count="$(awk 'END { print NR }' "$profile_file")"
[ "$line_count" = 5 ] || fail 'profile must contain exactly five canonical lines'
package_line="$(sed -n '1p' "$profile_file")"
user_line="$(sed -n '2p' "$profile_file")"
base_line="$(sed -n '3p' "$profile_file")"
preview_line="$(sed -n '4p' "$profile_file")"
module_line="$(sed -n '5p' "$profile_file")"

[ "$package_line" = "package = \"$expected_package\"" ] ||
    fail 'package does not match selected fixed profile'
[ "$user_line" = 'user = 0' ] || fail 'selected profiles are fixed to Android user 0'
[ "$base_line" = 'base_slot = "base"' ] || fail 'base_slot must be exactly base'
[ "$preview_line" = 'preview_slot = "preview"' ] ||
    fail 'preview_slot must be exactly preview'
[ "$module_line" = 'module = "uclone-slices-preview"' ] ||
    fail 'module must be exactly uclone-slices-preview'
package="$expected_package"
user=0
base_slot=base
preview_slot=preview
module=uclone-slices-preview
[ "$base_slot" != "$preview_slot" ] || fail 'base and preview slots must differ'

destination="$output_root/target-profile-$profile"
[ ! -L "$destination" ] || fail 'generated destination must not be a symlink'
staging="$(mktemp -d "$output_root/.target-profile-$profile.XXXXXX")"
cleanup() {
    if [ -n "${staging:-}" ] && [ -d "$staging" ]; then
        chmod -R u+w "$staging" 2>/dev/null || true
        rm -rf -- "$staging"
    fi
}
trap cleanup EXIT HUP INT TERM

module_root="/data/adb/modules/$module"
runtime_root="/data/adb/$module"
bridge_classpath="$module_root/runtime/slot-bridge.apk"
fsprobe_path="$module_root/runtime/slot-fsprobe"
slotctl_path="$module_root/bin/slotctl"
runtime_socket="$runtime_root/run/ucloned.sock"
runtime_lock="$runtime_root/run/runtime.lock"
rescue_journal_root="$runtime_root/rescue-journal"
canonical_ce_root="/data/user/$user"
canonical_de_root="/data/user_de/$user"
target_ce="$canonical_ce_root/$package"
target_de="$canonical_de_root/$package"
ce_slot_root="/data/misc_ce/$user/$module/slots"
de_slot_root="/data/misc_de/$user/$module/slots"
target_ce_slot_root="$ce_slot_root/$package"
target_de_slot_root="$de_slot_root/$package"
preview_ce="$target_ce_slot_root/$preview_slot"
preview_de="$target_de_slot_root/$preview_slot"
staging_ce="$target_ce_slot_root/.$preview_slot.staging"
staging_de="$target_de_slot_root/.$preview_slot.staging"
process_regex="^${package//./[.]}(:|$)"

printf '%s\n' \
    "profile=$profile" \
    "package=$package" \
    "user=$user" \
    "base_slot=$base_slot" \
    "preview_slot=$preview_slot" \
    "module=$module" \
    "module_root=$module_root" \
    "runtime_root=$runtime_root" \
    "bridge_classpath=$bridge_classpath" \
    "fsprobe_path=$fsprobe_path" \
    "slotctl_path=$slotctl_path" \
    "runtime_socket=$runtime_socket" \
    "runtime_lock=$runtime_lock" \
    "rescue_journal_root=$rescue_journal_root" \
    "canonical_ce_root=$canonical_ce_root" \
    "canonical_de_root=$canonical_de_root" \
    "target_ce=$target_ce" \
    "target_de=$target_de" \
    "ce_slot_root=$ce_slot_root" \
    "de_slot_root=$de_slot_root" \
    "target_ce_slot_root=$target_ce_slot_root" \
    "target_de_slot_root=$target_de_slot_root" \
    "preview_ce=$preview_ce" \
    "preview_de=$preview_de" \
    "staging_ce=$staging_ce" \
    "staging_de=$staging_de" \
    >"$staging/target-profile.properties"

printf '%s\n' \
    '#!/system/bin/sh' \
    "readonly UCLONE_TARGET_PROFILE='$profile'" \
    "readonly UCLONE_TARGET_PACKAGE='$package'" \
    "readonly UCLONE_TARGET_USER='$user'" \
    "readonly UCLONE_BASE_SLOT='$base_slot'" \
    "readonly UCLONE_PREVIEW_SLOT='$preview_slot'" \
    "readonly UCLONE_MODULE='$module'" \
    "readonly UCLONE_MODULE_ROOT='$module_root'" \
    "readonly UCLONE_RUNTIME_ROOT='$runtime_root'" \
    "readonly UCLONE_TARGET_PROCESS_REGEX='$process_regex'" \
    'export UCLONE_TARGET_PROFILE UCLONE_TARGET_PACKAGE UCLONE_TARGET_USER' \
    'export UCLONE_BASE_SLOT UCLONE_PREVIEW_SLOT UCLONE_MODULE' \
    'export UCLONE_MODULE_ROOT UCLONE_RUNTIME_ROOT UCLONE_TARGET_PROCESS_REGEX' \
    >"$staging/target-profile.sh"

printf '%s\n' \
    '/// Selected canonical target profile name.' \
    "pub const PROFILE: &str = \"$profile\";" \
    '/// Compiled Android target package.' \
    "pub const PACKAGE: &str = \"$package\";" \
    '/// Compiled Android target user.' \
    "pub const USER_ID: u32 = $user;" \
    '/// Compiled Android target user as a path segment.' \
    "pub const USER_ID_STR: &str = \"$user\";" \
    '/// Compiled native base slot identifier.' \
    "pub const BASE_SLOT: &str = \"$base_slot\";" \
    '/// Compiled alternate preview slot identifier.' \
    "pub const PREVIEW_SLOT: &str = \"$preview_slot\";" \
    '/// Compiled `KernelSU` module identifier.' \
    "pub const MODULE: &str = \"$module\";" \
    '/// Fixed `KernelSU` module root.' \
    "pub const MODULE_ROOT: &str = \"$module_root\";" \
    '/// Fixed runtime state root.' \
    "pub const RUNTIME_ROOT: &str = \"$runtime_root\";" \
    '/// Fixed `app_process` bridge classpath.' \
    "pub const BRIDGE_CLASSPATH: &str = \"$bridge_classpath\";" \
    '/// Fixed native filesystem-policy helper path.' \
    "pub const FSPROBE_PATH: &str = \"$fsprobe_path\";" \
    '/// Fixed command-line client path.' \
    "pub const SLOTCTL_PATH: &str = \"$slotctl_path\";" \
    '/// Fixed runtime Unix socket.' \
    "pub const RUNTIME_SOCKET: &str = \"$runtime_socket\";" \
    '/// Fixed runtime/rescue lock.' \
    "pub const RUNTIME_LOCK: &str = \"$runtime_lock\";" \
    '/// Fixed rescue journal root.' \
    "pub const RESCUE_JOURNAL_ROOT: &str = \"$rescue_journal_root\";" \
    '/// Canonical CE root for the compiled user.' \
    "pub const CANONICAL_CE_ROOT: &str = \"$canonical_ce_root\";" \
    '/// Canonical DE root for the compiled user.' \
    "pub const CANONICAL_DE_ROOT: &str = \"$canonical_de_root\";" \
    '/// Canonical CE target path.' \
    "pub const TARGET_CE: &str = \"$target_ce\";" \
    '/// Canonical DE target path.' \
    "pub const TARGET_DE: &str = \"$target_de\";" \
    '/// Root of compiled CE slot storage.' \
    "pub const CE_SLOT_ROOT: &str = \"$ce_slot_root\";" \
    '/// Root of compiled DE slot storage.' \
    "pub const DE_SLOT_ROOT: &str = \"$de_slot_root\";" \
    '/// Parent of the compiled target CE slots.' \
    "pub const TARGET_CE_SLOT_ROOT: &str = \"$target_ce_slot_root\";" \
    '/// Parent of the compiled target DE slots.' \
    "pub const TARGET_DE_SLOT_ROOT: &str = \"$target_de_slot_root\";" \
    '/// Compiled target ready CE preview path.' \
    "pub const PREVIEW_CE: &str = \"$preview_ce\";" \
    '/// Compiled target ready DE preview path.' \
    "pub const PREVIEW_DE: &str = \"$preview_de\";" \
    '/// Compiled target CE staging path.' \
    "pub const STAGING_CE: &str = \"$staging_ce\";" \
    '/// Compiled target DE staging path.' \
    "pub const STAGING_DE: &str = \"$staging_de\";" \
    >"$staging/target_profile.rs"

printf '%s\n' \
    '#ifndef UCLONE_TARGET_PROFILE_H' \
    '#define UCLONE_TARGET_PROFILE_H' \
    "#define UCLONE_TARGET_PROFILE \"$profile\"" \
    "#define UCLONE_TARGET_PACKAGE \"$package\"" \
    "#define UCLONE_TARGET_USER $user" \
    "#define UCLONE_TARGET_USER_STRING \"$user\"" \
    "#define UCLONE_BASE_SLOT \"$base_slot\"" \
    "#define UCLONE_PREVIEW_SLOT \"$preview_slot\"" \
    "#define UCLONE_MODULE \"$module\"" \
    "#define UCLONE_TARGET_CE \"$target_ce\"" \
    "#define UCLONE_TARGET_DE \"$target_de\"" \
    "#define UCLONE_CE_SLOT_ROOT \"$ce_slot_root\"" \
    "#define UCLONE_DE_SLOT_ROOT \"$de_slot_root\"" \
    '#endif' \
    >"$staging/target_profile.h"

for component in bridge controller; do
    case "$component" in
        bridge) java_package=com.uclone.slotbridge ;;
        controller) java_package=com.uclone.slotpreview.controller ;;
    esac
    java_dir="$staging/java/$component/$(printf '%s' "$java_package" | tr . /)"
    mkdir -p "$java_dir"
    printf '%s\n' \
        "package $java_package;" \
        '' \
        'final class TargetProfile {' \
        "    static final String PROFILE = \"$profile\";" \
        "    static final String PACKAGE = \"$package\";" \
        "    static final int USER_ID = $user;" \
        "    static final String BASE_SLOT = \"$base_slot\";" \
        "    static final String PREVIEW_SLOT = \"$preview_slot\";" \
        "    static final String MODULE = \"$module\";" \
        "    static final String MODULE_ROOT = \"$module_root\";" \
        "    static final String TARGET_CE = \"$target_ce\";" \
        "    static final String TARGET_DE = \"$target_de\";" \
        "    static final String BRIDGE_CLASSPATH = \"$bridge_classpath\";" \
        "    static final String SLOTCTL_PATH = \"$slotctl_path\";" \
        '' \
        '    private TargetProfile() {}' \
        '}' \
        >"$java_dir/TargetProfile.java"
done

find "$staging" -type f -exec chmod 0444 {} +
find "$staging" -type d -exec chmod 0755 {} +
if [ -e "$destination" ]; then
    [ -d "$destination" ] || fail 'generated destination is not a directory'
    if ! diff -qr "$destination" "$staging" >/dev/null; then
        fail 'generated destination is stale; use a clean output root'
    fi
    cleanup
    staging=''
else
    mv "$staging" "$destination"
    staging=''
fi
printf '%s\n' "$destination"
