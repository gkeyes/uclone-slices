#!/usr/bin/env bash

set -euo pipefail
umask 077

if (( $# == 0 )); then
    PROFILE=slotprobe
elif (( $# == 2 )) && [ "$1" = --profile ]; then
    PROFILE=$2
else
    printf '%s\n' 'usage: tools/build-slot-runtime-android.sh [--profile slotprobe|fitness|generic]' >&2
    exit 2
fi
case "$PROFILE" in slotprobe|fitness|generic) ;; *) exit 2 ;; esac

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)"
RUNTIME_ROOT="$REPO_ROOT/slot-runtime"
TOOLCHAIN_BIN="/Users/jianchen/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin"
CARGO="$TOOLCHAIN_BIN/cargo"
NDK_ROOT="/usr/local/share/android-commandlinetools/ndk/29.0.14206865"
TARGET="aarch64-linux-android"
LINKER="$NDK_ROOT/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android29-clang"
EVIDENCE_DIR="$REPO_ROOT/.omo/evidence/slices-preview-validation"
LOG_PATH="$EVIDENCE_DIR/rust-android-aarch64-$PROFILE.log"
PROFILE_TARGET_DIR="$RUNTIME_ROOT/target-profiles/$PROFILE"
PROFILE_OUTPUT="$PROFILE_TARGET_DIR/profile"
export PATH="$TOOLCHAIN_BIN:/usr/bin:/bin:/usr/sbin:/sbin"

require_file() {
    path="$1"
    [ -f "$path" ] || { printf 'missing file: %s\n' "$path" >&2; exit 1; }
    [ ! -L "$path" ] || { printf 'symlink rejected: %s\n' "$path" >&2; exit 1; }
}

require_executable() {
    require_file "$1"
    [ -x "$1" ] || { printf 'not executable: %s\n' "$1" >&2; exit 1; }
}

require_executable "$CARGO"
require_executable "$LINKER"
require_file "$NDK_ROOT/source.properties"
grep -Eq '^Pkg\.Revision = 29\.' "$NDK_ROOT/source.properties" || {
    printf '%s\n' 'Android NDK 29 is required' >&2
    exit 1
}
require_file "$RUNTIME_ROOT/.cargo/config.toml"
grep -Fq "linker = \"$LINKER\"" "$RUNTIME_ROOT/.cargo/config.toml" || {
    printf '%s\n' 'Rust target linker is not the fixed NDK 29 linker' >&2
    exit 1
}

mkdir -p "$EVIDENCE_DIR"
mkdir -p "$PROFILE_OUTPUT"
GENERATED_PROFILE="$PROFILE_OUTPUT/target-profile-$PROFILE"
if [ -d "$GENERATED_PROFILE" ]; then
    chmod -R u+w "$GENERATED_PROFILE"
    rm -rf -- "$GENERATED_PROFILE"
fi
"$REPO_ROOT/tools/render-target-profile.sh" "$PROFILE" "$PROFILE_OUTPUT" >/dev/null
(
    cd "$RUNTIME_ROOT"
    export CARGO_TARGET_DIR="$PROFILE_TARGET_DIR"
    export CARGO_BUILD_JOBS=1
    export UCLONE_TARGET_PROFILE="$PROFILE"
    unset CARGO_BUILD_TARGET RUSTFLAGS RUSTC_WRAPPER
    printf 'repository=%s\n' "$REPO_ROOT"
    printf 'rust_toolchain=1.95.0\n'
    printf 'target=%s\n' "$TARGET"
    printf 'profile=%s\n' "$PROFILE"
    printf 'ndk=%s\n' "$NDK_ROOT"
    printf 'command='; printf '%q ' "$CARGO" build --locked --offline --release --target "$TARGET" --bins
    printf '\n'
    CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 \
        "$CARGO" build --locked --offline --release --target "$TARGET" --bins
) >"$LOG_PATH" 2>&1

for binary in ucloned slotctl; do
    artifact="$PROFILE_TARGET_DIR/$TARGET/release/$binary"
    require_executable "$artifact"
done

printf '%s\n' "PASS: Android $TARGET Rust binaries built with fixed NDK 29 (see $LOG_PATH)"
