#!/usr/bin/env bash

set -euo pipefail
umask 077

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RENDERER="$ROOT_DIR/tools/render-target-profile.sh"

fail() {
    printf 'profile consistency error: %s\n' "$1" >&2
    exit 1
}

extract_package() {
    component="$1"
    source_file="$2"
    package="$3"
    [ -n "$package" ] || fail "could not read $component target from $source_file"
    printf '%s=%s\n' "$component" "$package"
}

legacy_consistency_red() {
    values="$(
        extract_package runtime slot-runtime/src/protocol.rs \
            "$(sed -n 's/^pub const ALLOWED_PACKAGE: &str = "\([a-z0-9._]*\)";/\1/p' \
                "$ROOT_DIR/slot-runtime/src/protocol.rs")"
        extract_package bridge slot-bridge/PackagePolicy.java \
            "$(sed -n 's/^    static final String ALLOWED_PACKAGE = "\([a-z0-9._]*\)";/\1/p' \
                "$ROOT_DIR/slot-bridge/src/main/java/com/uclone/slotbridge/PackagePolicy.java")"
        extract_package fsprobe slot-fsprobe/path_policy.c \
            "$(sed -n 's/^#define PACKAGE "\([a-z0-9._]*\)"/\1/p' \
                "$ROOT_DIR/slot-fsprobe/src/path_policy.c")"
        extract_package kernelsu slot-kernelsu/emergency-containment.sh \
            "$(sed -n 's/^PACKAGE=\([a-z0-9._]*\)$/\1/p' \
                "$ROOT_DIR/slot-kernelsu/emergency-containment.sh")"
        extract_package controller slot-preview-controller/PreviewCommand.java \
            "$(sed -n 's/^    static final String TARGET_PACKAGE = "\([a-z0-9._]*\)";/\1/p' \
                "$ROOT_DIR/slot-preview-controller/src/main/java/com/uclone/slotpreview/controller/PreviewCommand.java")"
    )"
    printf '%s\n' "$values"
    packages="$(printf '%s\n' "$values" | cut -d= -f2 | sort -u)"
    package_count="$(printf '%s\n' "$packages" | sed '/^$/d' | wc -l | tr -d ' ')"
    if [ "$package_count" -ne 1 ]; then
        fail "production consumers disagree: $(printf '%s' "$packages" | tr '\n' ' ')"
    fi
    fail 'canonical build-time renderer is missing'
}

assert_file_contains() {
    file="$1"
    expected="$2"
    grep -F -x "$expected" "$file" >/dev/null ||
        fail "$file is missing exact generated value: $expected"
}

assert_source_contains() {
    file="$1"
    expected="$2"
    grep -F "$expected" "$ROOT_DIR/$file" >/dev/null ||
        fail "$file is not wired to the generated target profile"
}

verify_production_wiring() {
    assert_source_contains slot-runtime/build.rs 'UCLONE_TARGET_PROFILE'
    assert_source_contains slot-bridge/build.gradle.kts 'generatedTargetProfile.resolve("java/bridge")'
    assert_source_contains slot-bridge/src/main/java/com/uclone/slotbridge/PackagePolicy.java \
        'static final int ALLOWED_USER_ID = 0;'
    assert_source_contains slot-fsprobe/src/path_policy.c '#include "target_profile.h"'
    assert_source_contains slot-kernelsu/emergency-containment.sh 'uclone_load_profile "$PROFILE_FILE"'
    assert_source_contains slot-preview-controller/build.gradle.kts \
        'generatedTargetProfile.resolve("java/controller")'
    assert_source_contains slot-preview-controller/src/main/java/com/uclone/slotpreview/controller/PreviewCommand.java \
        'static final String TARGET_PACKAGE = TargetProfile.PACKAGE;'
    assert_source_contains tools/package-kernelsu-preview.sh \
        'artifact target profile mismatch:'

    stale="$(
        while IFS= read -r rust_file; do
            [ "${rust_file##*/}" != tests.rs ] || continue
            awk '/^#\[cfg\(test\)\]/{exit} /com[.](uclone[.]slotprobe|asksky[.]fitness)/{
                print FILENAME ":" FNR ":" $0
            }' "$rust_file"
        done < <(find "$ROOT_DIR/slot-runtime/src" -type f -name '*.rs')
        {
        find "$ROOT_DIR/slot-bridge/src/main" -type f -name '*.java'
        find "$ROOT_DIR/slot-fsprobe/src" -type f \( -name '*.c' -o -name '*.h' \)
        find "$ROOT_DIR/slot-kernelsu" -maxdepth 1 -type f -name '*.sh' \
            ! -name 'profile-loader.sh'
        find "$ROOT_DIR/slot-preview-controller/src/main" -type f \
            \( -name '*.java' -o -name '*.xml' \)
        } | xargs grep -nE 'com\.(uclone\.slotprobe|asksky\.fitness)' || true
    )"
    [ -z "$stale" ] || fail "production target literal remains: $stale"
    printf '%s\n' 'PASS production consumers use generated target constants'
}

file_mode() {
    if mode="$(stat -f '%Lp' "$1" 2>/dev/null)"; then
        printf '%s\n' "$mode"
    else
        stat -c '%a' "$1"
    fi
}

verify_rendered_profile() {
    profile="$1"
    package="$2"
    output_root="$3"
    mkdir -p "$output_root"
    "$RENDERER" "$profile" "$output_root"
    generated="$output_root/target-profile-$profile"
    [ -d "$generated" ] || fail "$profile render directory is missing"
    [ ! -L "$generated" ] || fail "$profile render directory is a symlink"
    assert_file_contains "$generated/target-profile.properties" "profile=$profile"
    assert_file_contains "$generated/target-profile.properties" "package=$package"
    assert_file_contains "$generated/target-profile.properties" 'user=0'
    assert_file_contains "$generated/target-profile.properties" 'base_slot=base'
    assert_file_contains "$generated/target-profile.properties" 'preview_slot=preview'
    assert_file_contains "$generated/target-profile.properties" 'module=uclone-slices-preview'
    [ "$(file_mode "$generated/target-profile.sh")" = 444 ] ||
        fail "$profile generated shell profile is mutable"
    sh -n "$generated/target-profile.sh"
    printf 'PASS rendered profile=%s package=%s user=0 base_slot=base preview_slot=preview module=uclone-slices-preview\n' \
        "$profile" "$package"
}

expect_rejected() {
    label="$1"
    shift
    if "$@" >"$scratch/rejected.stdout" 2>"$scratch/rejected.stderr"; then
        fail "$label was accepted"
    fi
    printf 'PASS rejected %s\n' "$label"
}

fixture_rejects_value() {
    label="$1"
    expression="$2"
    fixture="$scratch/fixture-$label"
    mkdir -p "$fixture/tools" "$fixture/slot-targets" "$fixture/output"
    cp "$RENDERER" "$fixture/tools/render-target-profile.sh"
    chmod 0755 "$fixture/tools/render-target-profile.sh"
    sed "$expression" "$ROOT_DIR/slot-targets/slotprobe.toml" \
        >"$fixture/slot-targets/slotprobe.toml"
    expect_rejected "$label profile value" \
        "$fixture/tools/render-target-profile.sh" slotprobe "$fixture/output"
}

verify_adversarial_boundaries() {
    mkdir -p "$scratch/output" "$scratch/real-output"
    expect_rejected 'foreign profile name' "$RENDERER" foreign "$scratch/output"
    expect_rejected 'traversal profile name' "$RENDERER" ../fitness "$scratch/output"
    expect_rejected 'absolute profile name' "$RENDERER" /tmp/fitness "$scratch/output"
    expect_rejected 'caller supplied path argument' \
        "$RENDERER" fitness "$scratch/output" /data/user/0/com.asksky.fitness
    ln -s "$scratch/real-output" "$scratch/symlink-output"
    expect_rejected 'symlink output root' "$RENDERER" fitness "$scratch/symlink-output"
    ln -s "$scratch/real-output" "$scratch/output/target-profile-slotprobe"
    expect_rejected 'symlink generated destination' "$RENDERER" slotprobe "$scratch/output"

    fixture="$scratch/fixture-symlink-profile"
    mkdir -p "$fixture/tools" "$fixture/slot-targets" "$fixture/output"
    cp "$RENDERER" "$fixture/tools/render-target-profile.sh"
    chmod 0755 "$fixture/tools/render-target-profile.sh"
    ln -s "$ROOT_DIR/slot-targets/slotprobe.toml" \
        "$fixture/slot-targets/slotprobe.toml"
    expect_rejected 'symlink profile file' \
        "$fixture/tools/render-target-profile.sh" slotprobe "$fixture/output"

    fixture_rejects_value package \
        's/^package = .*/package = "com.foreign.app"/'
    fixture_rejects_value user 's/^user = .*/user = 10/'
    fixture_rejects_value base-slot \
        's/^base_slot = .*/base_slot = "foreign"/'
    fixture_rejects_value preview-slot \
        's/^preview_slot = .*/preview_slot = "foreign"/'
    fixture_rejects_value module \
        's/^module = .*/module = "..\/foreign"/'
    printf '%s\n' 'PASS adversarial target-profile boundaries'
}

main() {
    mode="${1:-}"
    if [ "$#" -gt 1 ] || { [ -n "$mode" ] && [ "$mode" != '--adversarial' ]; }; then
        fail 'usage is test-target-profiles.sh [--adversarial]'
    fi
    if [ ! -x "$RENDERER" ]; then
        if [ "$mode" = '--adversarial' ]; then
            fail 'canonical build-time renderer is missing for boundary checks'
        fi
        legacy_consistency_red
    fi
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/uclone-target-profile-test.XXXXXX")"
    trap 'chmod -R u+w "$scratch" 2>/dev/null || true; rm -rf "$scratch"' \
        EXIT HUP INT TERM
    verify_rendered_profile slotprobe com.uclone.slotprobe "$scratch/slotprobe"
    verify_rendered_profile fitness com.asksky.fitness "$scratch/fitness"
    verify_rendered_profile generic com.uclone.slots.preview "$scratch/generic"
    verify_production_wiring
    if [ "$mode" = '--adversarial' ]; then
        verify_adversarial_boundaries
    fi
    printf '%s\n' 'PASS target profiles are consistent'
}

main "$@"
