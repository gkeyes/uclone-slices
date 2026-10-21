#!/usr/bin/env python3
"""Read DEX declarations from a saved ROM services.jar; does not modify a device."""

import json
import struct
import sys
import zipfile

TARGETS = {
    "Lcom/android/server/wm/ActivityStartInterceptor;",
    "Lcom/android/server/wm/ActivityStarter$Request;",
    "Lcom/android/server/wm/ActivityRecord;",
    "Lcom/android/server/wm/TaskFragment;",
    "Lcom/android/server/wm/RootWindowContainer;",
    "Lcom/android/server/wm/ActivityTaskManagerService;",
    "Lcom/android/server/wm/ActivityTaskSupervisor;",
    "Lcom/android/server/am/ActivityManagerService;",
    "Lcom/android/server/am/ProcessRecord;",
}
METHODS = {
    "intercept", "completeResumeLocked", "resumeTopActivity", "topRunningActivity",
    "attachApplicationLocked", "onSystemReady", "getTopDisplayFocusedRootTask",
    "startActivityFromRecents", "getTask", "getDisplayId", "isActivityTypeHome",
}


def declarations(data):
    def u32(offset):
        return struct.unpack_from("<I", data, offset)[0]

    def uleb(offset):
        result, shift = 0, 0
        while True:
            byte = data[offset]
            offset += 1
            result |= (byte & 127) << shift
            if not byte & 128:
                return result, offset
            shift += 7

    strings = []
    for index in range(u32(56)):
        offset = u32(u32(60) + index * 4)
        _, offset = uleb(offset)
        strings.append(data[offset:data.index(0, offset)].decode("utf-8", "replace"))
    types = [strings[u32(u32(68) + index * 4)] for index in range(u32(64))]
    prototypes = []
    for index in range(u32(72)):
        offset = u32(76) + index * 12
        parameters = u32(offset + 8)
        args = [] if not parameters else [
            types[struct.unpack_from("<H", data, parameters + 4 + arg * 2)[0]]
            for arg in range(u32(parameters))
        ]
        prototypes.append({"parameters": args, "returns": types[u32(offset + 4)]})
    fields = []
    for index in range(u32(80)):
        owner, field_type, name = struct.unpack_from("<HHI", data, u32(84) + index * 8)
        fields.append({"class": types[owner], "name": strings[name], "type": types[field_type]})
    methods = []
    for index in range(u32(88)):
        owner, prototype, name = struct.unpack_from("<HHI", data, u32(92) + index * 8)
        methods.append({"class": types[owner], "name": strings[name], **prototypes[prototype]})
    result = []
    for index in range(u32(96)):
        offset = u32(100) + index * 32
        owner = types[u32(offset)]
        position = u32(offset + 24)
        if owner not in TARGETS or not position:
            continue
        counts = []
        for _ in range(4):
            value, position = uleb(position)
            counts.append(value)
        declared_fields, declared_methods = [], []
        for count in counts[:2]:
            entry_index = 0
            for _ in range(count):
                delta, position = uleb(position)
                _, position = uleb(position)
                entry_index += delta
                declared_fields.append(fields[entry_index])
        for count in counts[2:]:
            entry_index = 0
            for _ in range(count):
                delta, position = uleb(position)
                _, position = uleb(position)
                _, position = uleb(position)
                entry_index += delta
                if methods[entry_index]["name"] in METHODS:
                    declared_methods.append(methods[entry_index])
        result.append({"class": owner, "fields": declared_fields, "methods": declared_methods})
    return result


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: inspect-framework.py /path/to/services.jar")
    output = []
    with zipfile.ZipFile(sys.argv[1]) as jar:
        for name in jar.namelist():
            if name.endswith(".dex"):
                output.extend(declarations(jar.read(name)))
    print(json.dumps(output, indent=2))
