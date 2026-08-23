#!/usr/bin/env bash
set -euo pipefail

EXPECTED_CERT_SHA256=3a98013499c588855ac936d884d9e55497f72827c91ca01e50cf9ca4fc290648
APKSIGNER=${APKSIGNER:?APKSIGNER is required}

[ "$#" -eq 1 ]
[ -x "$APKSIGNER" ]
[ -f "$1" ]

DIGESTS=$(
    "$APKSIGNER" verify --verbose --print-certs "$1" |
        sed -n 's/^Signer #[0-9][0-9]* certificate SHA-256 digest: //p' |
        tr '[:upper:]' '[:lower:]'
)
DIGEST_COUNT=$(printf '%s\n' "$DIGESTS" | sed '/^$/d' | wc -l | tr -d ' ')
[ "$DIGEST_COUNT" = 1 ]
[ "$DIGESTS" = "$EXPECTED_CERT_SHA256" ]
printf '%s certificate SHA-256: %s\n' "$(basename "$1")" "$DIGESTS"
