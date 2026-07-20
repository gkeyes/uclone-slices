#![doc = "Production fixed-command and read-only Android system adapters."]
#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures select only the adapter surface under test"
)]

mod command;
mod executor;
mod facts;
mod filesystem;
mod io;
mod namespace;
mod parse;
mod probe;
mod procfs;

pub use command::SystemCommandRunner;
pub use executor::{
    ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput, StdProcessExecutor,
};
pub use facts::{FactError, ProcessSet, StdSystemFacts, SystemFacts};
pub use probe::SystemPackageProbe;

#[cfg(test)]
#[allow(
    unused_imports,
    reason = "integration fixtures select individual constructors"
)]
pub(super) use command::{
    bind_invocation_for_paths, force_stop_invocation, unmount_invocation_for_target,
};
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "integration fixtures select individual fact helpers"
)]
pub(super) use filesystem::{mirror_inodes_at, slot_inodes_at};
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "integration fixture inspects one namespace command"
)]
pub(super) use namespace::namespace_stat_invocation;
#[cfg(test)]
pub(super) fn arm64_zygote_processes_at(
    proc_root: &std::path::Path,
) -> Result<ProcessSet, FactError> {
    procfs::arm64_zygote_processes_at(proc_root)
}

/// Maximum stdout accepted from any fixed non-bridge child process.
pub const MAX_PROCESS_OUTPUT_BYTES: usize = 16 * 1024;
/// Fixed deadline applied to every non-bridge child process.
pub const PROCESS_TIMEOUT_SECONDS: u64 = 5;

/// `PackageManager` persisted-inode policy for this bridge-backed adapter.
///
/// [`SystemPackageProbe`] accepts only the bridge's non-zero persisted CE/DE inode pair. It never
/// substitutes canonical-path inodes when that bridge fact is missing or malformed. The lifecycle
/// guard compares this pair with the enrolled native base alongside version, code path, and
/// pending-session state.
pub const PACKAGE_MANAGER_INODE_POLICY: &str =
    "bridge persisted CE/DE inodes required; canonical fallback forbidden";
