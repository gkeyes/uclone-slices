#!/usr/bin/env python3
"""Exercise the production native executables in disposable Linux namespaces."""

import ctypes
import os
import signal
import select
import time
import sys

sys.excepthook = sys.__excepthook__

libc = ctypes.CDLL(None, use_errno=True)
libc.mount.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_ulong, ctypes.c_char_p]
libc.unshare.argtypes = [ctypes.c_int]
CLONE_NEWNS = 0x20000
MS_BIND, MS_REC, MS_SLAVE, MS_SHARED = 4096, 16384, 1 << 19, 1 << 20
PACKAGE = "com.example.app"
MODULE = "/data/adb/modules/uclone-slices-v2/bin"
RUNTIME = "/data/adb/uclone-slices-v2"
VIEW = MODULE + "/uclone-process-view"
ANCHOR = MODULE + "/uclone-namespace-anchor"
HELPER = RUNTIME + "/manager-helper/test/uclone_archive"
CE = ["/data/user/0/", "/data/data/", "/data_mirror/data_ce/null/0/", "/data_mirror/data_ce/0/"]
DE = ["/data/user_de/0/", "/data_mirror/data_de/null/0/", "/data_mirror/data_de/0/"]


def checked(result):
    if result != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))


def mount(source, target, flags=MS_BIND, filesystem=None):
    checked(libc.mount(source.encode() if source else None, target.encode(),
                       filesystem.encode() if filesystem else None, flags, None))


def private_namespace():
    checked(libc.unshare(CLONE_NEWNS))
    mount(None, "/", MS_SLAVE | MS_REC)


def read(path):
    with open(path, "rb") as stream:
        return stream.read().decode().strip()


def identity(pid):
    fields = read(f"/proc/{pid}/stat").rsplit(")", 1)[1].split()
    return fields[19]


def slot_source(domain, slot):
    return f"/data/misc_{domain}/0/uclone-slices-v2/slots/{PACKAGE}/{slot}"


def bind_account(slot):
    for domain, paths in [("ce", CE), ("de", DE)]:
        for prefix in paths[:1]:
            mount(slot_source(domain, slot), prefix + PACKAGE)


def execute(arguments, runtime_slot=None):
    output, writer = os.pipe()
    pid = os.fork()
    if pid == 0:
        try:
            os.close(output)
            os.dup2(writer, 1)
            os.close(writer)
            if runtime_slot:
                private_namespace()
                bind_account(runtime_slot)
            os.execv(arguments[0], arguments)
        except BaseException as error:
            os.write(2, ("namespace exec failed: " + str(error) + "\n").encode())
            os._exit(125)
    os.close(writer)
    chunks = []
    deadline = time.monotonic() + 5  # Same whole-frame bound as native control.
    while True:
        remaining = deadline - time.monotonic()
        assert remaining > 0 and select.select([output], [], [], remaining)[0], "native helper kept its output pipe open"
        chunk = os.read(output, 4096)
        if not chunk:
            break
        chunks.append(chunk)
    os.close(output)
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status), b"".join(chunks).decode().strip()


def application(masked=False, mixed=False):
    commands, sender = os.pipe()
    response, writer = os.pipe()
    pid = os.fork()
    if pid == 0:
        try:
            os.close(sender)
            os.close(response)
            private_namespace()
            old_ce = os.open(CE[0] + PACKAGE + "/value", os.O_RDONLY)
            old_de = os.open(DE[0] + PACKAGE + "/value", os.O_RDONLY)
            if mixed:
                mount(slot_source("ce", "slot-1"), CE[0] + PACKAGE)
            if masked:
                mount("tmpfs", "/data", 0, "tmpfs")
                mount("tmpfs", "/data_mirror", 0, "tmpfs")
            os.write(writer, b"ready\n")
            while os.read(commands, 1):
                values = [read(prefix + PACKAGE + "/value") if os.path.exists(prefix + PACKAGE + "/value")
                          else "masked" for prefix in CE + DE]
                values += [os.pread(fd, 64, 0).decode().strip() for fd in (old_ce, old_de)]
                os.write(writer, (",".join(values) + "\n").encode())
            os._exit(0)
        except BaseException as error:
            os.write(2, ("synthetic App setup failed: " + str(error) + "\n").encode())
            os._exit(125)
    os.close(commands)
    os.close(writer)
    reader = os.fdopen(response, "rb", buffering=0)
    assert reader.readline() == b"ready\n"
    return pid, sender, reader


def snapshot(app):
    os.write(app[1], b"?")
    return app[2].readline().decode().strip().split(",")


def install(app, slot, *, started=None, uid="0", inherited=False, isolated=False):
    args = [VIEW, str(app[0]), uid, started or identity(app[0]), PACKAGE, slot, "--install"]
    if inherited:
        args.append("--verify-inherited")
    if isolated:
        args.append("--isolated")
    # Runtime already has slot-2 mounted. Base must come from PID 1, not Runtime.
    return execute(args, runtime_slot="slot-2")[0]


def stop(app):
    os.close(app[1])
    app[2].close()
    os.kill(app[0], signal.SIGTERM)
    os.waitpid(app[0], 0)


def main():
    assert os.getuid() == 0 and os.getpid() == 1
    lab = sys.argv[1]
    layout = sys.argv[2]
    root = lab + "/root-" + layout
    for prefix in CE + DE:
        directory = root + prefix + PACKAGE
        os.makedirs(directory)
        with open(directory + "/value", "w") as stream:
            stream.write("base-ce" if prefix in CE else "base-de")
    for domain in ("ce", "de"):
        for slot in ("slot-1", "slot-2"):
            directory = root + slot_source(domain, slot)
            os.makedirs(directory)
            with open(directory + "/value", "w") as stream:
                stream.write(slot + "-" + domain)
    for destination, source in [(MODULE + "/uclone-process-view", "process-view"),
                                (MODULE + "/uclone-namespace-anchor", "namespace-anchor"),
                                (HELPER, "helper")]:
        os.makedirs(os.path.dirname(root + destination), exist_ok=True)
        with open(lab + "/" + source, "rb") as stream:
            binary = stream.read()
        with open(root + destination, "wb") as stream:
            stream.write(binary)
        os.chmod(root + destination, 0o700)
    os.chmod(root + RUNTIME, 0o700)
    os.makedirs(root + "/proc")
    os.makedirs(root + "/dev")
    with open(root + "/dev/null", "wb"):
        pass
    mount("/dev/null", root + "/dev/null")
    # Propagation flags operate on mountpoints, not ordinary chroot directories.
    mount(root, root)
    os.chroot(root)
    os.chdir("/")
    mount("proc", "/proc", 0, "proc")
    if layout != "separate":
        # Real Android /data is shared; canonical/legacy/mirror paths can name
        # the same dentry through bind mounts (or a symlink on another ROM).
        mount("/data", "/data")
        mount(None, "/data", MS_SHARED)
        if layout == "symlink":
            os.rename("/data/user/0", "/data/user/fixture-unused")
            os.symlink("/data/data", "/data/user/0")
        else:
            mount("/data/data", "/data/user/0")
        for prefix in CE[2:]:
            mount("/data/data", prefix.rstrip("/"))
        for prefix in DE[1:]:
            mount("/data/user_de/0", prefix.rstrip("/"))
    first, sibling = application(), application()
    init_mounts = read("/proc/1/mountinfo")
    sibling_mounts = read(f"/proc/{sibling[0]}/mountinfo")
    base = ["base-ce"] * len(CE) + ["base-de"] * len(DE) + ["base-ce", "base-de"]
    assert snapshot(first) == snapshot(sibling) == base
    assert install(first, "slot-1", started=str(int(identity(first[0])) + 1)) != 0
    assert install(first, "slot-1", uid="10123") != 0
    assert snapshot(first) == base
    assert install(first, "slot-1") == 0
    selected = ["slot-1-ce"] * len(CE) + ["slot-1-de"] * len(DE) + ["base-ce", "base-de"]
    assert snapshot(first) == selected
    assert read("/proc/1/mountinfo") == init_mounts, "App account mounts escaped into init"
    assert read(f"/proc/{sibling[0]}/mountinfo") == sibling_mounts, "App account mounts escaped into sibling"
    mounts = [line.split() for line in read(f"/proc/{first[0]}/mountinfo").splitlines()]
    for prefix in CE + DE:
        target = os.path.realpath(prefix + PACKAGE)
        assert sum(fields[4] == target for fields in mounts) == 1, f"alias stacked mounts at {target}"
    assert snapshot(sibling) == base
    assert read(CE[0] + PACKAGE + "/value") == "base-ce"
    assert install(first, "slot-1", inherited=True) == 0
    assert install(first, "base", inherited=True) != 0
    assert snapshot(first) == selected
    assert install(first, "base") == 0
    assert snapshot(first) == base
    mixed = application(mixed=True)
    mixed_mounts = read(f"/proc/{mixed[0]}/mountinfo")
    assert install(mixed, "slot-1", inherited=True) != 0
    assert install(mixed, "base", inherited=True) != 0
    assert read(f"/proc/{mixed[0]}/mountinfo") == mixed_mounts, "verification modified an App view"
    stop(mixed)
    isolated = application(masked=True)
    assert install(isolated, "slot-1") != 0
    assert install(isolated, "slot-1", isolated=True) == 0
    assert snapshot(isolated) == ["masked"] * (len(CE) + len(DE)) + ["base-ce", "base-de"]
    stop(isolated)
    assert execute([VIEW, "1", "0", identity(1), PACKAGE, "slot-1", "--install"])[0] != 0
    assert execute([ANCHOR, "--start"])[0] != 0
    assert execute([ANCHOR, "--start"], runtime_slot="slot-1")[0] == 0
    status, anchor_pid = execute([ANCHOR, "--check"])
    assert status == 0 and anchor_pid.isdigit()
    # The original Runtime caller has exited. The anchor pins its view and the helper
    # adopts both its mount namespace and root before resolving canonical paths.
    assert execute([ANCHOR, "--exec-helper", HELPER]) == (0, "slot-1-ce\nslot-1-de")
    os.kill(int(anchor_pid), signal.SIGTERM)
    os.waitpid(int(anchor_pid), 0)
    assert execute([ANCHOR, "--check"])[0] != 0
    assert execute([ANCHOR, "--start"], runtime_slot="slot-2")[0] != 0
    assert read("/proc/1/mountinfo") == init_mounts, "later verification escaped into init"
    assert read(f"/proc/{sibling[0]}/mountinfo") == sibling_mounts
    stop(first)
    stop(sibling)
    print(layout + ": native namespace checks passed: paired views, aliases, old FDs, Base, isolation, identity, anchor recovery")


if __name__ == "__main__":
    main()
