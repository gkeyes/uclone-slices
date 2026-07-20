#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures inspect bounded fact constants"
)]

use crate::android::MountCounts;
use crate::domain::{DataInodes, PackageName, SlotId};

use super::{filesystem, procfs};

pub const MAX_PROCESS_COUNT: usize = 4096;
const FIRST_APPLICATION_UID: u32 = 10_000;
const PER_USER_UID_RANGE: u32 = 100_000;

/// Failure to obtain one trustworthy fixed Android system fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FactError {
    /// A required kernel or filesystem observation could not be read.
    #[error("Android system fact source is unavailable")]
    Unavailable,
    /// An observation was malformed, unsafe, incomplete, or outside the fixed target.
    #[error("Android system fact is invalid")]
    Invalid,
}

/// A sorted, bounded set of unique non-zero Linux process identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSet {
    pids: Vec<u32>,
}

impl ProcessSet {
    /// Validates and canonicalizes at most 4096 unique non-zero process identifiers.
    pub fn new(mut pids: Vec<u32>) -> Result<Self, FactError> {
        if pids.len() > MAX_PROCESS_COUNT || pids.contains(&0) {
            return Err(FactError::Invalid);
        }
        pids.sort_unstable();
        if pids.windows(2).any(|pair| pair.first() == pair.last()) {
            return Err(FactError::Invalid);
        }
        Ok(Self { pids })
    }

    /// Returns the deterministic ascending process identifiers.
    pub fn pids(&self) -> &[u32] {
        &self.pids
    }
}

/// Read-only boundary for fixed user-zero filesystem, mount, and process facts.
pub trait SystemFacts: core::fmt::Debug {
    /// Returns the daemon and PID 1 mount-namespace identifiers.
    fn mount_namespace_ids(&mut self) -> Result<(u64, u64), FactError>;

    /// Returns exact canonical user-zero CE and DE directory inodes.
    fn canonical_inodes(&mut self, package: &PackageName) -> Result<DataInodes, FactError>;

    /// Returns the unique matching CE and DE data-mirror directory inodes.
    fn mirror_inodes(&mut self, package: &PackageName) -> Result<DataInodes, FactError>;

    /// Counts mountinfo entries targeting the exact canonical CE and DE paths.
    fn canonical_mount_counts(&mut self, package: &PackageName) -> Result<MountCounts, FactError>;

    /// Returns a complete non-base slot inode pair, or none when both paths are absent.
    fn slot_inodes(
        &mut self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<Option<DataInodes>, FactError>;

    /// Returns every process owned by the fixed package UID after name validation.
    fn package_processes(
        &mut self,
        package: &PackageName,
        uid: u32,
    ) -> Result<ProcessSet, FactError>;

    /// Returns all ARM64 Zygote processes used by this ARM64-only Preview target.
    fn arm64_zygote_processes(&mut self) -> Result<ProcessSet, FactError>;
}

/// Production zero-sized reader for fixed Android kernel and filesystem facts.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdSystemFacts;

impl StdSystemFacts {
    /// Constructs the zero-sized production fact reader.
    pub const fn new() -> Self {
        Self
    }
}

impl SystemFacts for StdSystemFacts {
    fn mount_namespace_ids(&mut self) -> Result<(u64, u64), FactError> {
        procfs::mount_namespace_ids()
    }

    fn canonical_inodes(&mut self, package: &PackageName) -> Result<DataInodes, FactError> {
        filesystem::canonical_inodes(package)
    }

    fn mirror_inodes(&mut self, package: &PackageName) -> Result<DataInodes, FactError> {
        filesystem::mirror_inodes(package)
    }

    fn canonical_mount_counts(&mut self, package: &PackageName) -> Result<MountCounts, FactError> {
        filesystem::canonical_mount_counts(package)
    }

    fn slot_inodes(
        &mut self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<Option<DataInodes>, FactError> {
        filesystem::slot_inodes(package, slot)
    }

    fn package_processes(
        &mut self,
        package: &PackageName,
        uid: u32,
    ) -> Result<ProcessSet, FactError> {
        if !(FIRST_APPLICATION_UID..PER_USER_UID_RANGE).contains(&uid) {
            return Err(FactError::Invalid);
        }
        procfs::package_processes(package, uid)
    }

    fn arm64_zygote_processes(&mut self) -> Result<ProcessSet, FactError> {
        procfs::arm64_zygote_processes()
    }
}
