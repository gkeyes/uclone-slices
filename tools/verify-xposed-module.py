#!/usr/bin/env python3
"""Verify the modern Xposed registration and entry classes in the built APK."""

import struct
import sys
import zipfile
import json
from pathlib import Path


def dex_classes(data):
    def u32(offset):
        return struct.unpack_from("<I", data, offset)[0]

    def uleb(offset):
        value, shift = 0, 0
        while True:
            byte = data[offset]
            offset += 1
            value |= (byte & 127) << shift
            if not byte & 128:
                return value, offset
            shift += 7

    strings = []
    for index in range(u32(56)):
        offset = u32(u32(60) + index * 4)
        _, offset = uleb(offset)
        strings.append(data[offset:data.index(0, offset)].decode("utf-8", "replace"))
    types = [strings[u32(u32(68) + index * 4)] for index in range(u32(64))]
    method_names = [strings[u32(u32(92) + index * 8 + 4)] for index in range(u32(88))]
    classes = {}
    for index in range(u32(96)):
        offset = u32(100) + index * 32
        superclass = u32(offset + 8)
        position = u32(offset + 24)
        names = set()
        if position:
            counts = []
            for _ in range(4):
                value, position = uleb(position)
                counts.append(value)
            for _ in range(counts[0] + counts[1]):
                _, position = uleb(position)
                _, position = uleb(position)
            for count in counts[2:]:
                method = 0
                for _ in range(count):
                    delta, position = uleb(position)
                    _, position = uleb(position)
                    _, position = uleb(position)
                    method += delta
                    names.add(method_names[method])
        classes[types[u32(offset)]] = (
            None if superclass == 0xffffffff else types[superclass], names,
        )
    return classes


def verify(path):
    path = Path(path)
    if path.is_dir():
        # AGP names unsigned PR artifacts differently from fixed-signed deliveries.
        # Read its output manifest so both builds check their actual Release APK.
        outputs = json.loads((path / "output-metadata.json").read_text())["elements"]
        assert len(outputs) == 1, "expected one universal Manager APK"
        candidate = path / outputs[0]["outputFile"]
        assert candidate.resolve().parent == path.resolve(), "APK path escapes output directory"
        path = candidate
    with zipfile.ZipFile(path) as apk:
        assert "assets/xposed_init" not in apk.namelist(), "legacy entry is still packaged"
        properties = dict(
            line.split("=", 1)
            for line in apk.read("META-INF/xposed/module.prop").decode().splitlines()
            if line.strip() and not line.startswith("#")
        )
        assert properties["minApiVersion"] == properties["targetApiVersion"] == "102"
        assert properties["staticScope"] == "true"
        assert properties["autoHotReload"] == "false", "live launch state must not be silently replaced"
        assert apk.read("META-INF/xposed/scope.list").decode().split() == ["system"]
        entries = apk.read("META-INF/xposed/java_init.list").decode().split()
        assert len(entries) == 1, "one system coordinator entry is required"
        classes = {}
        for name in apk.namelist():
            if name.startswith("classes") and name.endswith(".dex"):
                data = apk.read(name)
                assert b"de/robv/android/xposed/" not in data, "legacy Xposed reference in DEX"
                classes.update(dex_classes(data))
        assert not any(name.startswith("Lio/github/libxposed/api/") for name in classes), "framework API was bundled"
        entry = "L" + entries[0].replace(".", "/") + ";"
        superclass, methods = classes[entry]
        assert superclass == "Lio/github/libxposed/api/XposedModule;"
        assert {"<init>", "onModuleLoaded", "onSystemServerStarting"}.issubset(methods)
    print("Modern Xposed API 102 verified: entry, system scope, lifecycle, no bundled/legacy API")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify-xposed-module.py /path/to/manager.apk-or-output-directory")
    verify(sys.argv[1])
