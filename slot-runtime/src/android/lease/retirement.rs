use std::fs;
use std::io;
use std::path::Path;

use crate::atomic_file::sync_directory;

use super::GateLeaseError;

pub(super) fn retire(
    root: &Path,
    lease: &Path,
    tombstone: &Path,
    retired: &Path,
    already_tombstoned: bool,
) -> Result<(), GateLeaseError> {
    retire_with_sync(
        root,
        lease,
        tombstone,
        retired,
        already_tombstoned,
        sync_directory,
    )
}

fn retire_with_sync<F>(
    root: &Path,
    lease: &Path,
    tombstone: &Path,
    retired: &Path,
    already_tombstoned: bool,
    mut sync: F,
) -> Result<(), GateLeaseError>
where
    F: FnMut(&Path) -> io::Result<()>,
{
    if !already_tombstoned {
        fs::rename(lease, tombstone).map_err(|_| GateLeaseError::Unavailable)?;
        sync(root).map_err(|_| GateLeaseError::Unavailable)?;
    }
    fs::rename(tombstone, retired).map_err(|_| GateLeaseError::Unavailable)?;
    sync(root).map_err(|_| GateLeaseError::Unavailable)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "temporary retirement fixtures")]
mod tests {
    use super::*;

    #[test]
    fn pre_unlink_sync_failure_leaves_the_snapshot_in_the_tombstone() {
        let directory = tempfile::tempdir().unwrap();
        let lease = directory.path().join("package.gate");
        let tombstone = directory.path().join(".package.gate.retiring");
        let retired = directory.path().join(".package.gate.retired");
        fs::write(&lease, b"exact snapshot").unwrap();

        let result = retire_with_sync(
            directory.path(),
            &lease,
            &tombstone,
            &retired,
            false,
            |_| Err(io::Error::other("injected sync failure")),
        );

        assert_eq!(result, Err(GateLeaseError::Unavailable));
        assert!(!lease.exists());
        assert_eq!(fs::read(tombstone).unwrap(), b"exact snapshot");
    }

    #[test]
    fn post_unlink_sync_failure_never_reports_retirement_success() {
        let directory = tempfile::tempdir().unwrap();
        let lease = directory.path().join("package.gate");
        let tombstone = directory.path().join(".package.gate.retiring");
        let retired = directory.path().join(".package.gate.retired");
        fs::write(&lease, b"exact snapshot").unwrap();
        let mut sync_calls = 0_u8;

        let result = retire_with_sync(
            directory.path(),
            &lease,
            &tombstone,
            &retired,
            false,
            |_| {
                sync_calls += 1;
                if sync_calls == 1 {
                    Ok(())
                } else {
                    Err(io::Error::other("injected final sync failure"))
                }
            },
        );

        assert_eq!(result, Err(GateLeaseError::Unavailable));
        assert!(!lease.exists());
        assert!(!tombstone.exists());
        assert_eq!(fs::read(retired).unwrap(), b"exact snapshot");
    }

    #[test]
    fn successful_retirement_preserves_the_exact_snapshot_as_retired_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let lease = directory.path().join("package.gate");
        let tombstone = directory.path().join(".package.gate.retiring");
        let retired = directory.path().join(".package.gate.retired");
        fs::write(&lease, b"exact snapshot").unwrap();

        retire_with_sync(
            directory.path(),
            &lease,
            &tombstone,
            &retired,
            false,
            |_| Ok(()),
        )
        .unwrap();

        assert!(!lease.exists());
        assert!(!tombstone.exists());
        assert_eq!(fs::read(retired).unwrap(), b"exact snapshot");
    }
}
