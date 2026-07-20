# slot-fsprobe

`slot-fsprobe` is the audited native boundary for reading one fscrypt directory policy. The Rust
daemon remains safe Rust and invokes this binary only as:

```text
slot-fsprobe policy <path>
```

Success writes exactly one lowercase SHA-256 digest plus `\n` to standard output. Failure is
silent and uses a stable nonzero exit code; paths and `errno` values are never logged.

The combined materializer security proof has three independent inputs: this helper first gates
the opened directory with `fstat(2)` (type, user-0 app UID/GID, and write bits), then reads the
fscrypt policy with the fixed ioctl; the Rust materializer separately reads the root SELinux
context with its fixed `getfattr` command. Keeping SELinux outside this tiny native helper avoids
adding another path parser or a shell boundary while still requiring stat, fscrypt, and SELinux
evidence before a slot is accepted.

## Fixed path contract

The only canonical paths are:

```text
/data/user/0/com.uclone.slotprobe
/data/user_de/0/com.uclone.slotprobe
```

The only slot forms are one ready or staging directory directly below these fixed package roots:

```text
/data/misc_ce/0/uclone-slices-preview/slots/com.uclone.slotprobe/<slot>
/data/misc_de/0/uclone-slices-preview/slots/com.uclone.slotprobe/<slot>
```

`<slot>` is 1–64 bytes, begins with `a-z`, and otherwise contains only `a-z`, `0-9`, `-`, or `_`.
The runtime staging spelling `.<slot>.staging` is also accepted. Descendants, aliases such as
`/data/data`, other users/packages/roots, empty segments, `.`/`..`, trailing slashes, relative paths,
and paths longer than 4095 bytes are rejected before `open(2)`.

The helper must run as root. It opens the final component with
`O_RDONLY|O_DIRECTORY|O_CLOEXEC|O_NOFOLLOW`, then `fstat(2)` requires an ordinary user-0 app-owned
directory (UID 10000–19999, matching GID, and no group/other write bit). It never creates, writes,
renames, deletes, mounts, shells out, or accesses the network.

## Policy proof

The sole kernel operation is `FS_IOC_GET_ENCRYPTION_POLICY_EX`. Policy versions other than v1 or
v2, or a returned size inconsistent with that version, fail closed. The digest input is:

```text
little_endian_u64(policy_size) || exactly policy_size returned policy bytes
```

The SHA-256 implementation is bundled, allocation-free, and covered by published FIPS vectors.
No policy bytes, key identifiers, path, or kernel diagnostics are printed.

## Stable exit codes

| Code | Meaning |
|---:|---|
| `0` | Digest written successfully |
| `2` | CLI shape rejected |
| `3` | Path rejected |
| `4` | Caller is not root |
| `5` | Directory open failed |
| `6` | Directory metadata rejected |
| `7` | Policy read/shape or descriptor close failed |
| `8` | Output write failed |

## Local verification

Host-only validation does not issue an ioctl:

```bash
make -C slot-fsprobe test
```

The Android gate requires NDK 29 and builds only arm64-v8a/API 29:

```bash
ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/29.0.14206865" \
  make -C slot-fsprobe android-check
```

Compiler warnings are errors. The Android executable is PIE and links with stack-protector,
FORTIFY, RELRO, and immediate binding flags. Building does not install or execute anything on a
device.

## Threat model

The caller may supply malformed CLI bytes and paths, and filesystem state may change between path
validation and open. Fixed roots, a single allowlisted package, exact depth, `O_NOFOLLOW`, and
post-open metadata checks reduce the boundary to an already-existing app data root. The fixed
parents are assumed to remain root-controlled Android data directories. A process already able to
replace those parents or alter kernel ioctl results is outside this helper's trust boundary.

The digest is evidence for equality checks, not authentication. The daemon must keep its existing
execution gate, inode, SELinux/MCS, and lifecycle checks; this helper does not replace them.
