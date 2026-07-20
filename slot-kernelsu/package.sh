#!/usr/bin/env bash

set -euo pipefail
umask 077

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
exec "$ROOT_DIR/tools/package-kernelsu-preview.sh" "$@"
