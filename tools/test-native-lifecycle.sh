#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
LAB=$(mktemp -d)
trap 'sudo rm -rf -- "$LAB"' EXIT
JDK=${JAVA_HOME:?JAVA_HOME is required for the public JNI header}
g++ -std=c++20 -O2 -Wall -Wextra -Werror -static -pthread \
    -DUCLONE_BUILD_ID=\"lifecycle-test\" \
    -I"$JDK/include" -I"$JDK/include/linux" \
    "$ROOT/tools/test-native-lifecycle.cpp" -o "$LAB/lifecycle"
for scenario in accept no_exemption reject wrong_uid disconnect timeout; do
    mkdir "$LAB/$scenario"
    # No host /data or Android package is accessed. PID 1 exiting also reaps the
    # fixture companion/Runtime peers; timeout bounds a broken bootstrap protocol.
    sudo timeout -s KILL 20 unshare --mount --pid --net --fork \
        "$LAB/lifecycle" "$LAB/$scenario" "$scenario"
done
