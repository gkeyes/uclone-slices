#!/usr/bin/env bash
set -euo pipefail

EXPECTED_CERT_SHA256=4883794fda44a6ea085eae09ea2ead48e5233c67e76f751108fb6469b412ba14
APKSIGNER=${APKSIGNER:?APKSIGNER is required}

[ "$#" -ge 2 ]
[ -x "$APKSIGNER" ]

MANAGER_APK=$1
[ -f "$MANAGER_APK" ]

certificate_digest() {
    local digests
    local digest_count
    digests=$("$APKSIGNER" verify --verbose --print-certs "$1" |
        sed -n 's/^Signer #[0-9][0-9]* certificate SHA-256 digest: //p' |
        tr '[:upper:]' '[:lower:]')
    digest_count=$(printf '%s\n' "$digests" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$digest_count" != 1 ]; then
        printf 'expected one APK signer, found %s: %s\n' "$digest_count" "$1" >&2
        return 1
    fi
    printf '%s\n' "$digests"
}

MANAGER_DIGEST=$(certificate_digest "$MANAGER_APK")
[ -n "$MANAGER_DIGEST" ]
printf '%s certificate SHA-256: %s\n' "$(basename "$MANAGER_APK")" "$MANAGER_DIGEST"
[ "$MANAGER_DIGEST" = "$EXPECTED_CERT_SHA256" ]

shift
for HOOK_APK in "$@"; do
    [ -f "$HOOK_APK" ]
    HOOK_DIGEST=$(certificate_digest "$HOOK_APK")
    [ -n "$HOOK_DIGEST" ]
    printf '%s certificate SHA-256: %s\n' "$(basename "$HOOK_APK")" "$HOOK_DIGEST"
    [ "$HOOK_DIGEST" = "$EXPECTED_CERT_SHA256" ]
    [ "$MANAGER_DIGEST" = "$HOOK_DIGEST" ]
done
