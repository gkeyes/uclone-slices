#!/usr/bin/env bash
set -euo pipefail

EXPECTED_CERT_SHA256=4883794fda44a6ea085eae09ea2ead48e5233c67e76f751108fb6469b412ba14
APKSIGNER=${APKSIGNER:?APKSIGNER is required}

[ "$#" -ge 2 ]
[ -x "$APKSIGNER" ]

MANAGER_APK=$1
[ -f "$MANAGER_APK" ]

certificate_digest() {
    DIGESTS=$("$APKSIGNER" verify --verbose --print-certs "$1" |
        sed -n 's/^Signer #[0-9][0-9]* certificate SHA-256 digest: //p' |
        tr '[:upper:]' '[:lower:]')
    [ "$(printf '%s\n' "$DIGESTS" | sed '/^$/d' | wc -l | tr -d ' ')" = 1 ]
    printf '%s\n' "$DIGESTS"
}

MANAGER_DIGEST=$(certificate_digest "$MANAGER_APK")
[ -n "$MANAGER_DIGEST" ]
[ "$MANAGER_DIGEST" = "$EXPECTED_CERT_SHA256" ]

shift
for HOOK_APK in "$@"; do
    [ -f "$HOOK_APK" ]
    HOOK_DIGEST=$(certificate_digest "$HOOK_APK")
    [ -n "$HOOK_DIGEST" ]
    [ "$HOOK_DIGEST" = "$EXPECTED_CERT_SHA256" ]
    [ "$MANAGER_DIGEST" = "$HOOK_DIGEST" ]
done
