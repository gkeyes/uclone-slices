#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
KERNELSU_ROOT="$REPO_ROOT/slot-kernelsu"
RUNTIME_ROOT="$REPO_ROOT/slot-runtime"
PROFILE=slotprobe
TARGET_ROOT="$REPO_ROOT/.omo/evidence/slices-preview-package"
die() { printf '%s\n' "$1" >&2; exit 1; }
while (( $# > 0 )); do
    case "$1" in
        --profile) [ "$#" -ge 2 ] || exit 2; PROFILE=$2; shift 2 ;;
        --output) [ "$#" -ge 2 ] || exit 2; TARGET_ROOT=$2; shift 2 ;;
        *) printf '%s\n' 'usage: tools/package-kernelsu-preview.sh [--profile slotprobe|fitness|generic] [--output directory]' >&2; exit 2 ;;
    esac
done
case "$PROFILE" in slotprobe|fitness|generic) ;; *) exit 2 ;; esac
BRIDGE_ROOT="$REPO_ROOT/slot-bridge/build/app-process/$PROFILE"
BRIDGE_APK="$BRIDGE_ROOT/slot-bridge.apk"
FSPROBE_ROOT="$REPO_ROOT/slot-fsprobe"
FSPROBE_BIN="$FSPROBE_ROOT/build/android/$PROFILE/bin/arm64-v8a/slot-fsprobe"
PROFILE_TARGET_DIR="$RUNTIME_ROOT/target-profiles/$PROFILE"
UCLONED_BIN="$PROFILE_TARGET_DIR/aarch64-linux-android/release/ucloned"
SLOTCTL_BIN="$PROFILE_TARGET_DIR/aarch64-linux-android/release/slotctl"
[ ! -L "$TARGET_ROOT" ] || { printf '%s\n' 'evidence root must not be a symlink' >&2; exit 1; }
[ ! -e "$TARGET_ROOT" ] || [ -d "$TARGET_ROOT" ] || die 'evidence root must be a directory'
mkdir -p "$TARGET_ROOT"
TARGET_ROOT=$(CDPATH= cd -- "$TARGET_ROOT" && pwd -P)
[ "$TARGET_ROOT" != "$REPO_ROOT" ] || exit 1
STAGING="$TARGET_ROOT/staging"
ZIP_PATH="$TARGET_ROOT/uclone-slices-preview-kernelsu.zip"
MANIFEST_PATH="$TARGET_ROOT/manifest.txt"
SHA256_PATH="$TARGET_ROOT/SHA256SUMS"
SUMMARY_PATH="$TARGET_ROOT/package-summary.txt"; ZIP_ENTRIES_PATH="$TARGET_ROOT/zip-entries.txt"
STAGED_SHA256_PATH="$TARGET_ROOT/staged-SHA256SUMS"; PROFILE_OUTPUT="$TARGET_ROOT/profile"
mkdir -p "$PROFILE_OUTPUT"
GENERATED_PROFILE="$PROFILE_OUTPUT/target-profile-$PROFILE"
"$REPO_ROOT/tools/render-target-profile.sh" "$PROFILE" "$PROFILE_OUTPUT" >/dev/null
. "$GENERATED_PROFILE/target-profile.sh"
[ ! -L "$STAGING" ] || { printf '%s\n' 'staging path must not be a symlink' >&2; exit 1; }
rm -rf "$STAGING"
mkdir -p "$STAGING/bin" "$STAGING/runtime"
require_regular_executable() {
    path="$1"
    [ -f "$path" ] || { printf 'missing file: %s\n' "$path" >&2; exit 1; }
    [ ! -L "$path" ] || { printf 'symlink rejected: %s\n' "$path" >&2; exit 1; }
    [ -x "$path" ] || { printf 'not executable: %s\n' "$path" >&2; exit 1; }
}
require_regular_file() {
    path="$1"
    [ -f "$path" ] || { printf 'missing file: %s\n' "$path" >&2; exit 1; }
    [ ! -L "$path" ] || { printf 'symlink rejected: %s\n' "$path" >&2; exit 1; }
}
file_mode() {
    if stat -f '%Lp' "$1" >/dev/null 2>&1; then
        stat -f '%Lp' "$1"
    else
        stat -c '%a' "$1"
    fi
}
require_regular_executable "$UCLONED_BIN"
require_regular_executable "$SLOTCTL_BIN"
require_regular_executable "$FSPROBE_BIN"
require_regular_file "$BRIDGE_APK"
require_regular_file "$GENERATED_PROFILE/target-profile.properties"
for attestation in \
    "$PROFILE_TARGET_DIR/profile/target-profile-$PROFILE/target-profile.properties" \
    "$BRIDGE_ROOT/target-profile.properties" \
    "$FSPROBE_ROOT/build/generated/$PROFILE/target-profile-$PROFILE/target-profile.properties"
do
    require_regular_file "$attestation"
    cmp -s "$GENERATED_PROFILE/target-profile.properties" "$attestation" || {
        printf 'artifact target profile mismatch: %s\n' "$attestation" >&2
        exit 1
    }
done
for script in customize.sh post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh boot-completed.sh boot-state.sh profile-loader.sh rescue.sh prepare-upgrade.sh upgrade-freeze.sh; do
    require_regular_file "$KERNELSU_ROOT/$script"
    /bin/sh -n "$KERNELSU_ROOT/$script"
    cp "$KERNELSU_ROOT/$script" "$STAGING/$script"
done
require_regular_file "$KERNELSU_ROOT/module.prop"
require_regular_file "$KERNELSU_ROOT/skip_mount"
[ ! -e "$KERNELSU_ROOT/sepolicy.rule" ] && [ ! -L "$KERNELSU_ROOT/sepolicy.rule" ] || die 'sepolicy.rule is not allowed in the Preview package'
grep -F -x "id=$UCLONE_MODULE" "$KERNELSU_ROOT/module.prop" >/dev/null || exit 1
PREVIEW_VERSION_NAME=${PREVIEW_VERSION_NAME:-0.3.0-preview.dev}
PREVIEW_VERSION_CODE=${PREVIEW_VERSION_CODE:-300000}
PREVIEW_BUILD_ID=${PREVIEW_BUILD_ID:-development}
case "$PREVIEW_VERSION_NAME" in *[!A-Za-z0-9._-]*|'') die 'invalid Preview version name' ;; esac
case "$PREVIEW_VERSION_CODE" in *[!0-9]*|'') die 'invalid Preview version code' ;; esac
case "$PREVIEW_BUILD_ID" in *[!A-Za-z0-9._-]*|'') die 'invalid Preview build id' ;; esac
sed \
    -e "s/^version=.*/version=$PREVIEW_VERSION_NAME/" \
    -e "s/^versionCode=.*/versionCode=$PREVIEW_VERSION_CODE/" \
    -e "s|^description=.*|description=Generic fail-closed KernelSU runtime for UClone Slots Preview; RPC v2 paired build $PREVIEW_BUILD_ID; no overlay or system partition changes|" \
    "$KERNELSU_ROOT/module.prop" >"$STAGING/module.prop"
cp "$KERNELSU_ROOT/skip_mount" "$STAGING/skip_mount"
cp "$GENERATED_PROFILE/target-profile.sh" "$STAGING/target-profile.sh"
cp "$UCLONED_BIN" "$STAGING/bin/ucloned"
cp "$SLOTCTL_BIN" "$STAGING/bin/slotctl"
cp "$FSPROBE_BIN" "$STAGING/runtime/slot-fsprobe"
cp "$BRIDGE_APK" "$STAGING/runtime/slot-bridge.apk"
: >"$STAGING/disable"
chmod 0700 "$STAGING/bin/ucloned" "$STAGING/bin/slotctl" "$STAGING/runtime/slot-fsprobe"
chmod 0700 "$STAGING/post-fs-data.sh" "$STAGING/startup-gate.sh" \
    "$STAGING/emergency-containment.sh" "$STAGING/service.sh" \
    "$STAGING/boot-completed.sh" "$STAGING/customize.sh" \
    "$STAGING/boot-state.sh" "$STAGING/profile-loader.sh" \
    "$STAGING/post-fs-setup.sh" "$STAGING/journal-packages.sh" \
    "$STAGING/rescue-retired-packages.sh" "$STAGING/rescue.sh" \
    "$STAGING/prepare-upgrade.sh" "$STAGING/upgrade-freeze.sh"
chmod 0644 "$STAGING/disable" "$STAGING/module.prop" "$STAGING/skip_mount"
chmod 0444 "$STAGING/target-profile.sh"
chmod 0600 "$STAGING/runtime/slot-bridge.apk"
for executable in \
    bin/ucloned bin/slotctl runtime/slot-fsprobe \
    post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh \
    boot-completed.sh boot-state.sh profile-loader.sh customize.sh rescue.sh prepare-upgrade.sh upgrade-freeze.sh
do
    [ "$(file_mode "$STAGING/$executable")" = 700 ] || {
        printf 'unexpected executable mode: %s\n' "$executable" >&2
        exit 1
    }
done
for private_file in runtime/slot-bridge.apk; do
    [ "$(file_mode "$STAGING/$private_file")" = 600 ] || {
        printf 'unexpected private-file mode: %s\n' "$private_file" >&2
        exit 1
    }
done
for metadata in disable module.prop skip_mount; do
    [ "$(file_mode "$STAGING/$metadata")" = 644 ] || {
        printf 'unexpected metadata mode: %s\n' "$metadata" >&2
        exit 1
    }
done
[ -f "$STAGING/disable" ] || exit 1
[ ! -L "$STAGING/disable" ] || exit 1
[ ! -s "$STAGING/disable" ] || die 'disable marker must be zero-length'
[ "$(file_mode "$STAGING/target-profile.sh")" = 444 ] || exit 1
find "$STAGING" -exec touch -t 200001010000 {} +
: >"$STAGED_SHA256_PATH"
while IFS= read -r file; do
    relative=${file#"$STAGING/"}
    digest=$(shasum -a 256 "$file" | awk '{print $1}')
    printf '%s  %s\n' "$digest" "$relative" >>"$STAGED_SHA256_PATH"
done < <(find "$STAGING" -type f -print | sort)
{
    printf 'UClone Slots Preview KernelSU package\n'
    printf 'repository=%s\n' "$REPO_ROOT"
    printf 'profile=%s\npackage=%s\nruntime_root=%s\n' \
        "$PROFILE" "$UCLONE_TARGET_PACKAGE" "$UCLONE_RUNTIME_ROOT"
    printf 'artifact\tmode\tbytes\tsha256\n'
    while IFS= read -r file; do
        relative=${file#"$STAGING/"}
        mode=$(file_mode "$file")
        bytes=$(wc -c <"$file" | tr -d ' ')
        digest=$(shasum -a 256 "$file" | awk '{print $1}')
        printf '%s\t%s\t%s\t%s\n' "$relative" "$mode" "$bytes" "$digest"
    done < <(find "$STAGING" -type f -print | sort)
} >"$MANIFEST_PATH"
rm -f "$ZIP_PATH"
(
    cd "$STAGING"
    TZ=UTC /usr/bin/zip -X -q -r "$ZIP_PATH" .
)
/usr/bin/unzip -tq "$ZIP_PATH"
VERIFY_ROOT="$TARGET_ROOT/verify-extract"
rm -rf "$VERIFY_ROOT"
mkdir -p "$VERIFY_ROOT"
/usr/bin/unzip -qq "$ZIP_PATH" -d "$VERIFY_ROOT"
[ -z "$(find "$VERIFY_ROOT" -type l -print -quit)" ] || die 'ZIP extraction produced a symlink'
for executable in \
    bin/ucloned bin/slotctl runtime/slot-fsprobe \
    post-fs-data.sh post-fs-setup.sh emergency-containment.sh journal-packages.sh rescue-retired-packages.sh startup-gate.sh service.sh \
    boot-completed.sh boot-state.sh profile-loader.sh rescue.sh prepare-upgrade.sh upgrade-freeze.sh
do
    [ "$(file_mode "$VERIFY_ROOT/$executable")" = 700 ] || {
        printf 'ZIP executable mode drift: %s\n' "$executable" >&2
        exit 1
    }
done
[ -f "$VERIFY_ROOT/disable" ] || die 'ZIP disable marker is missing'
[ ! -L "$VERIFY_ROOT/disable" ] || die 'ZIP disable marker is a symlink'
[ ! -s "$VERIFY_ROOT/disable" ] || {
    printf '%s\n' 'ZIP disable marker is not zero-length' >&2
    exit 1
}
[ "$(file_mode "$VERIFY_ROOT/disable")" = 644 ] || {
    printf '%s\n' 'ZIP disable marker mode drift' >&2
    exit 1
}
[ "$(file_mode "$VERIFY_ROOT/runtime/slot-bridge.apk")" = 600 ] || {
    printf '%s\n' 'ZIP bridge mode drift' >&2
    exit 1
}
rm -rf "$VERIFY_ROOT"
cat >"$ZIP_ENTRIES_PATH" <<'EOF'
bin/slotctl
bin/ucloned
boot-completed.sh
boot-state.sh
customize.sh
disable
emergency-containment.sh
journal-packages.sh
rescue-retired-packages.sh
module.prop
post-fs-data.sh
post-fs-setup.sh
prepare-upgrade.sh
profile-loader.sh
rescue.sh
runtime/slot-bridge.apk
runtime/slot-fsprobe
service.sh
skip_mount
startup-gate.sh
target-profile.sh
upgrade-freeze.sh
EOF
/usr/bin/unzip -Z1 "$ZIP_PATH" | awk '!/\/$/' | sort >"$TARGET_ROOT/actual-zip-entries.txt"
/usr/bin/cmp -s "$ZIP_ENTRIES_PATH" "$TARGET_ROOT/actual-zip-entries.txt" || {
    printf '%s\n' 'ZIP entries differ from the fixed KernelSU package contract' >&2
    exit 1
}
zip_digest=$(shasum -a 256 "$ZIP_PATH" | awk '{print $1}')
{
    printf '%s  %s\n' "$zip_digest" "$(basename "$ZIP_PATH")"
    while read -r digest relative; do
        printf '%s  staging/%s\n' "$digest" "$relative"
    done <"$STAGED_SHA256_PATH"
} >"$SHA256_PATH"
(
    cd "$STAGING"
    /usr/bin/shasum -a 256 -c "$STAGED_SHA256_PATH"
)
actual_zip_digest=$(shasum -a 256 "$ZIP_PATH" | awk '{print $1}')
[ "$actual_zip_digest" = "$zip_digest" ] || {
    printf '%s\n' 'ZIP checksum changed during packaging verification' >&2
    exit 1
}
(
    cd "$TARGET_ROOT"
    /usr/bin/shasum -a 256 -c "$SHA256_PATH"
)
{
    printf 'zip_sha256=%s\n--- manifest ---\n' "$zip_digest"
    cat "$MANIFEST_PATH"
} >"$SUMMARY_PATH"
printf '%s\n' "KernelSU Preview package: $ZIP_PATH"
printf '%s\n' "SHA256: $SHA256_PATH"
