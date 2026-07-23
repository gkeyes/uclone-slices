use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use crate::emergency_manifest::{RuntimeOwnerProof, RuntimeOwnerRole};

use super::RuntimeLockError;

mod live_process;
mod lock_directory;
mod owner_file;

use live_process::{current_boot_id, verify_live_process};
use lock_directory::PinnedLockDirectory;
use owner_file::{OwnerReadError, read_owner_record};

/// Result of a non-mutating Runtime owner proof check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeOwnerVerdict {
    /// The proof, lock artifact, owner record, and live process all match.
    Valid,
    /// No lock directory exists at the requested path.
    Missing,
    /// The artifact or live process cannot prove the supplied owner proof.
    Invalid(RuntimeOwnerMismatch),
}

/// Why a Runtime owner proof was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeOwnerMismatch {
    /// The persisted proof contains a zero identity field or wrong role.
    InvalidProof,
    /// The lock parent is a symlink or not a directory.
    UnsafeParent,
    /// The lock path is a symlink.
    LockSymlink,
    /// The lock path is not a directory.
    LockNotDirectory,
    /// The lock directory owner differs from its trusted parent (root on Android).
    LockOwner,
    /// The lock directory is not private mode `0700`.
    LockMode,
    /// The lock device differs from the persisted proof.
    LockDevice,
    /// The lock inode differs from the persisted proof.
    LockInode,
    /// The owner record is absent.
    OwnerMissing,
    /// The owner record is a symlink or non-regular file.
    OwnerArtifact,
    /// The owner record is not parent-owned/private mode `0600`.
    OwnerMetadata,
    /// The owner record cannot be parsed as the fixed four-line identity.
    OwnerMalformed,
    /// The owner record does not exactly equal the supplied proof.
    OwnerMismatch,
    /// The current boot epoch differs from the proof.
    BootId,
    /// The owner PID no longer exists.
    ProcessMissing,
    /// The owner PID has a different start time.
    ProcessStartTicks,
    /// The owner PID is not the exact `ucloned` command role.
    ProcessRole,
}

/// I/O or platform failure while reading a live owner proof.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeOwnerVerificationError {
    /// A platform identity probe failed before a verdict could be established.
    #[error("runtime owner platform probe: {0}")]
    Platform(#[source] RuntimeLockError),
    /// Metadata or file reading failed before a verdict could be established.
    #[error("runtime owner metadata I/O: {0}")]
    Io(#[from] io::Error),
}

/// Validates a fixed Runtime lock path against an emergency owner proof without mutation.
pub fn verify_runtime_owner(
    path: &Path,
    proof: &RuntimeOwnerProof,
) -> Result<RuntimeOwnerVerdict, RuntimeOwnerVerificationError> {
    if proof.pid() == 0
        || proof.start_ticks() == 0
        || proof.role() != RuntimeOwnerRole::Ucloned
        || proof.lock_device() == 0
        || proof.lock_inode() == 0
    {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::InvalidProof,
        ));
    }

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "runtime lock has no parent"))?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    let owner_uid = parent_metadata.uid();
    #[cfg(target_os = "android")]
    if owner_uid != 0 {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::UnsafeParent,
        ));
    }
    if parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
        || !private_mode(&parent_metadata, 0o700)
    {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::UnsafeParent,
        ));
    }

    let lock_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RuntimeOwnerVerdict::Missing);
        }
        Err(error) => return Err(error.into()),
    };
    if lock_metadata.file_type().is_symlink() {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockSymlink,
        ));
    }
    if !lock_metadata.is_dir() {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockNotDirectory,
        ));
    }
    if lock_metadata.uid() != owner_uid {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockOwner,
        ));
    }
    if !private_mode(&lock_metadata, 0o700) {
        return Ok(RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::LockMode));
    }
    if lock_metadata.dev() != proof.lock_device() {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockDevice,
        ));
    }
    if lock_metadata.ino() != proof.lock_inode() {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockInode,
        ));
    }

    let Some(pinned) = pin_lock_directory(path, &lock_metadata, owner_uid)? else {
        return Ok(RuntimeOwnerVerdict::Invalid(
            RuntimeOwnerMismatch::LockInode,
        ));
    };

    if let Some(mismatch) = verify_owner_record(&pinned, &lock_metadata, proof, owner_uid)? {
        return Ok(RuntimeOwnerVerdict::Invalid(mismatch));
    }

    let current_boot = current_boot_id()?;
    if current_boot != proof.boot_id().as_str() {
        return Ok(RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::BootId));
    }

    verify_live_process(proof)
}

fn pin_lock_directory(
    path: &Path,
    lock_metadata: &std::fs::Metadata,
    owner_uid: u32,
) -> Result<Option<PinnedLockDirectory>, RuntimeOwnerVerificationError> {
    let pinned = match PinnedLockDirectory::open(path) {
        Ok(pinned) => pinned,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !pinned.same_as(lock_metadata, owner_uid)? {
        return Ok(None);
    }
    Ok(Some(pinned))
}

fn verify_owner_record(
    pinned: &PinnedLockDirectory,
    lock_metadata: &std::fs::Metadata,
    proof: &RuntimeOwnerProof,
    owner_uid: u32,
) -> Result<Option<RuntimeOwnerMismatch>, RuntimeOwnerVerificationError> {
    let owner_result = read_owner_record(&pinned.owner_path(), owner_uid);
    if !pinned.same_after(lock_metadata, owner_uid)? {
        return Ok(Some(RuntimeOwnerMismatch::LockInode));
    }
    let owner = match owner_result {
        Ok(owner) => owner,
        Err(OwnerReadError::Missing) => return Ok(Some(RuntimeOwnerMismatch::OwnerMissing)),
        Err(OwnerReadError::Mismatch(reason)) => return Ok(Some(reason)),
        Err(OwnerReadError::Io(error)) => return Err(error.into()),
    };
    if owner.pid != proof.pid()
        || owner.boot != proof.boot_id().as_str()
        || owner.start_ticks != proof.start_ticks()
        || owner.role != "ucloned"
    {
        return Ok(Some(RuntimeOwnerMismatch::OwnerMismatch));
    }
    Ok(None)
}

fn private_mode(metadata: &std::fs::Metadata, expected: u32) -> bool {
    let mode = metadata.mode();
    mode & 0o777 == expected && mode & 0o7000 == 0
}

#[cfg(test)]
mod tests;
