#!/usr/bin/env bash

set -eu
set -o pipefail

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'usage: tools/validate-slices-preview.sh' >&2
    exit 2
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
RUNTIME_ROOT="$REPO_ROOT/slot-runtime"
BRIDGE_ROOT="$REPO_ROOT/slot-bridge"
FSPROBE_ROOT="$REPO_ROOT/slot-fsprobe"
PROBE_ROOT="$REPO_ROOT/slot-probe"
ANDROID_RUST_BUILD="$SCRIPT_DIR/build-slot-runtime-android.sh"
KERNELSU_BOOT_TEST="$SCRIPT_DIR/test-kernelsu-boot-flow.sh"
KERNELSU_UPGRADE_TEST="$SCRIPT_DIR/test-kernelsu-upgrade-gate.sh"
KERNELSU_UPGRADE_PROOF_TEST="$SCRIPT_DIR/test-kernelsu-upgrade-proof.sh"
EVIDENCE_DIR="$REPO_ROOT/.omo/evidence/slices-preview-validation"
RUN_LOG="$EVIDENCE_DIR/run.log"

mkdir -p "$EVIDENCE_DIR"
: >"$RUN_LOG"

trap 'rc=$?; printf "exit_code=%s\n" "$rc" >>"$RUN_LOG"; exit "$rc"' EXIT

log() {
    printf '%s\n' "$1" | tee -a "$RUN_LOG"
}

die() {
    log "FAIL: $1"
    exit 1
}

require_host_tool() {
    tool_name="$1"
    fixed_path="$2"
    [ -x "$fixed_path" ] && [ ! -L "$fixed_path" ] || {
        die "$tool_name is missing or symlinked at fixed path $fixed_path"
    }
}

CARGO="/Users/jianchen/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin/cargo"
RUST_TOOLCHAIN_BIN="/Users/jianchen/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin"
GRADLE="/Users/jianchen/.local/opt/gradle-8.13/bin/gradle"
MAKE="/usr/bin/make"
JAVA_HOME="/usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
ANDROID_HOME="/usr/local/share/android-commandlinetools"
ANDROID_NDK_HOME="$ANDROID_HOME/ndk/29.0.14206865"
require_host_tool cargo "$CARGO"
require_host_tool gradle "$GRADLE"
require_host_tool make "$MAKE"
require_host_tool java "$JAVA_HOME/bin/java"
require_host_tool rust_android_build "$ANDROID_RUST_BUILD"
require_host_tool kernelsu_boot_test "$KERNELSU_BOOT_TEST"
require_host_tool kernelsu_upgrade_test "$KERNELSU_UPGRADE_TEST"
require_host_tool kernelsu_upgrade_proof_test "$KERNELSU_UPGRADE_PROOF_TEST"
export PATH="$RUST_TOOLCHAIN_BIN:/usr/bin:/bin:/usr/sbin:/sbin"
export JAVA_HOME ANDROID_HOME ANDROID_NDK_HOME
export CARGO_BUILD_JOBS=1
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$RUNTIME_ROOT/target"

run_step() {
    step="$1"
    shift
    log_file="$EVIDENCE_DIR/$step.log"
    : >"$log_file"
    status=0
    {
        printf 'step=%s\n' "$step"
        printf 'command='
        printf '%q ' "$@"
        printf '\n'
        if "$@"; then
            printf 'result=PASS\n'
        else
            status=$?
            printf 'result=FAIL\nstatus=%s\n' "$status"
        fi
    } >"$log_file" 2>&1 || {
        log "FAIL: $step (see $log_file)"
        exit 1
    }
    if [ "$status" -ne 0 ]; then
        log "FAIL: $step (see $log_file)"
        exit 1
    fi
    log "PASS: $step (see $log_file)"
}

check_source_lines() {
    output="$EVIDENCE_DIR/source-lines.txt"
    : >"$output"
    failed=0
    source_roots=(
        "$RUNTIME_ROOT/src"
        "$BRIDGE_ROOT/src/main"
        "$PROBE_ROOT/src/main"
        "$FSPROBE_ROOT/src"
        "$REPO_ROOT/tools"
        "$REPO_ROOT/slot-kernelsu"
    )
    for source_root in "${source_roots[@]}"; do
        [ -d "$source_root" ] || {
            printf 'missing_source_root %s\n' "$source_root" >>"$output"
            failed=1
            continue
        }
        while IFS= read -r source_file; do
            case "$source_file" in
                *.rs|*.java|*.kt|*.kts|*.c|*.h|*.cc|*.cpp|*.sh|*.aidl) ;;
                *) continue ;;
            esac
            lines=$(wc -l <"$source_file" | tr -d ' ')
            printf '%s\t%s\n' "$lines" "${source_file#"$REPO_ROOT/"}" >>"$output"
            if [ "$lines" -gt 250 ]; then
                failed=1
            fi
        done < <(find "$source_root" -type f -print | sort)
    done
    if [ "$failed" -ne 0 ]; then
        return 1
    fi
}

run_rust_fmt() {
    cd "$RUNTIME_ROOT"
    "$CARGO" fmt --all -- --check
}

run_rust_check() {
    cd "$RUNTIME_ROOT"
    "$CARGO" check --offline --all-targets --all-features
}

run_rust_test() {
    cd "$RUNTIME_ROOT"
    "$CARGO" test --offline --all-targets --all-features
}

run_rust_clippy() {
    cd "$RUNTIME_ROOT"
    "$CARGO" clippy --offline --all-targets --all-features -- -D warnings
}

run_rust_doc() {
    cd "$RUNTIME_ROOT"
    RUSTDOCFLAGS='-D warnings' "$CARGO" doc --offline --no-deps --all-features
}

run_rust_android() {
    "$ANDROID_RUST_BUILD"
}

run_fsprobe_host() {
    "$MAKE" -C "$FSPROBE_ROOT" test
}

run_fsprobe_android() {
    [ -x "$ANDROID_NDK_HOME/ndk-build" ] || {
        printf 'Android NDK 29 not found: %s\n' "$ANDROID_NDK_HOME" >&2
        return 1
    }
    ANDROID_NDK_HOME="$ANDROID_NDK_HOME" "$MAKE" -C "$FSPROBE_ROOT" android-check
    artifact="$FSPROBE_ROOT/build/android/bin/arm64-v8a/slot-fsprobe"
    [ -f "$artifact" ] && [ ! -L "$artifact" ] && [ -x "$artifact" ] || {
        printf 'missing arm64-v8a fsprobe artifact: %s\n' "$artifact" >&2
        return 1
    }
}

run_bridge() {
    cd "$BRIDGE_ROOT"
    JAVA_HOME="$JAVA_HOME" ANDROID_HOME="$ANDROID_HOME" "$GRADLE" \
        --no-daemon --console=plain --max-workers=1 \
        testReleaseUnitTest lintRelease appProcessArtifact
}

run_probe() {
    cd "$REPO_ROOT"
    JAVA_HOME="$JAVA_HOME" ANDROID_HOME="$ANDROID_HOME" "$GRADLE" \
        --no-daemon --console=plain --max-workers=1 \
        :slot-probe:testDebugUnitTest \
        :slot-probe:assembleDebug \
        :slot-probe:assembleDebugAndroidTest \
        :slot-probe:lintDebug
}

log "Slices Preview validation started"
log "repository=$REPO_ROOT"
log "evidence=$EVIDENCE_DIR"
run_step source_line_limit check_source_lines
run_step kernelsu_boot_flow "$KERNELSU_BOOT_TEST"
run_step kernelsu_upgrade_gate "$KERNELSU_UPGRADE_TEST"
run_step kernelsu_upgrade_proof "$KERNELSU_UPGRADE_PROOF_TEST"
run_step rust_fmt run_rust_fmt
run_step rust_check run_rust_check
run_step rust_test run_rust_test
run_step rust_clippy run_rust_clippy
run_step rust_doc run_rust_doc
run_step rust_android_aarch64 run_rust_android
run_step fsprobe_host run_fsprobe_host
run_step fsprobe_android_arm64 run_fsprobe_android
run_step slot_bridge_standalone run_bridge
run_step slot_probe_gradle run_probe

{
    printf 'repository=%s\n' "$REPO_ROOT"
    printf 'rust_runtime=%s\n' "$RUNTIME_ROOT"
    printf 'bridge_artifact=%s\n' "$BRIDGE_ROOT/build/app-process/slot-bridge.apk"
    printf 'fsprobe_host=%s\n' "$FSPROBE_ROOT/build/host/slot-fsprobe-tests"
    printf 'fsprobe_arm64=%s\n' "$FSPROBE_ROOT/build/android/bin/arm64-v8a/slot-fsprobe"
    printf 'probe_apk=%s\n' "$PROBE_ROOT/build/outputs/apk/debug/slot-probe-debug.apk"
    printf 'probe_android_test_apk=%s\n' \
        "$PROBE_ROOT/build/outputs/apk/androidTest/debug/slot-probe-debug-androidTest.apk"
} >"$EVIDENCE_DIR/artifacts.txt"

log 'PASS: all local Slices Preview validation gates completed'
