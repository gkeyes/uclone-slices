#!/usr/bin/env bash
set -euo pipefail

EXPECTED_CERT_SHA256=3a98013499c588855ac936d884d9e55497f72827c91ca01e50cf9ca4fc290648
APKSIGNER=${APKSIGNER:?APKSIGNER is required}

fail() {
    printf 'verify-release-signing: %s\n' "$1" >&2
    exit 1
}

[ "$#" -eq 1 ] || fail "usage: verify-release-signing.sh APK"
[ -x "$APKSIGNER" ] || fail "apksigner not executable: $APKSIGNER"
[ -f "$1" ] || fail "APK not found: $1"

if ! VERIFY_OUTPUT=$("$APKSIGNER" verify --verbose --print-certs "$1" 2>&1); then
    printf '%s\n' "$VERIFY_OUTPUT" >&2
    fail "apksigner rejected $1"
fi
DIGESTS=$(
    printf '%s\n' "$VERIFY_OUTPUT" |
        sed -n 's/^Signer #[0-9][0-9]* certificate SHA-256 digest: //p' |
        tr '[:upper:]' '[:lower:]'
)
DIGEST_COUNT=$(printf '%s\n' "$DIGESTS" | sed '/^$/d' | wc -l | tr -d ' ')
[ "$DIGEST_COUNT" = 1 ] || fail "expected exactly one signer, found $DIGEST_COUNT"
[ "$DIGESTS" = "$EXPECTED_CERT_SHA256" ] || fail "unexpected signer certificate: $DIGESTS"
printf '%s certificate SHA-256: %s\n' "$(basename "$1")" "$DIGESTS"
