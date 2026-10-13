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
# apksigner 36 prints "Signer #1 certificate SHA-256 digest: ..."; apksigner 37 prints one
# "V2 Signer: certificate SHA-256 digest: ..." line per signature scheme instead.
SIGNER_COUNT=$(printf '%s\n' "$VERIFY_OUTPUT" | sed -n 's/^Number of signers: //p')
DIGESTS=$(
    printf '%s\n' "$VERIFY_OUTPUT" |
        sed -n -E 's/^(Signer #[0-9]+|V[0-9.]+ Signer( #[0-9]+)?:) certificate SHA-256 digest: //p' |
        tr '[:upper:]' '[:lower:]' |
        sort -u
)
DIGEST_COUNT=$(printf '%s\n' "$DIGESTS" | sed '/^$/d' | wc -l | tr -d ' ')
[ -z "$SIGNER_COUNT" ] || [ "$SIGNER_COUNT" = 1 ] || fail "expected exactly one signer, apksigner reports $SIGNER_COUNT"
[ "$DIGEST_COUNT" = 1 ] || { printf '%s\n' "$VERIFY_OUTPUT" >&2; fail "expected exactly one signer certificate, found $DIGEST_COUNT"; }
[ "$DIGESTS" = "$EXPECTED_CERT_SHA256" ] || fail "unexpected signer certificate: $DIGESTS"
printf '%s certificate SHA-256: %s\n' "$(basename "$1")" "$DIGESTS"
