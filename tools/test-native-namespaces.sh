#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
LAB=$(mktemp -d)
trap 'sudo rm -rf -- "$LAB"' EXIT
FLAGS=(-std=c++20 -O2 -Wall -Wextra -Werror -static -DUCLONE_BUILD_ID=\"namespace-test\")
g++ "${FLAGS[@]}" "$ROOT/native/process_view.cpp" -o "$LAB/process-view"
g++ "${FLAGS[@]}" "$ROOT/native/namespace_anchor.cpp" -o "$LAB/namespace-anchor"
cat >"$LAB/helper.cpp" <<'CPP'
#include <fstream>
#include <iostream>
#include <string>
int main() {
  for (const char* path : {"/data/user/0/com.example.app/value", "/data/user_de/0/com.example.app/value"}) {
    std::ifstream input(path);
    std::string value;
    if (!(input >> value)) return 1;
    std::cout << value << '\n';
  }
}
CPP
g++ "${FLAGS[@]}" "$LAB/helper.cpp" -o "$LAB/helper"
mkdir "$LAB/root"
# PID 1 belongs to this disposable namespace. All fixed production paths resolve
# inside the synthetic chroot; no host /data, App, or account is read or modified.
sudo unshare --mount --pid --fork -- python3 "$ROOT/tools/test-native-namespaces.py" "$LAB"
