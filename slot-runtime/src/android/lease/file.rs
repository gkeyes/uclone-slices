use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::atomic_file::{sync_directory, write_new_synced};
use crate::domain::PackageName;

use super::filesystem::{ensure_gate_root, existing_gate_root, path_present, read_secure_lease};
use super::retirement;
use super::{
    EmergencyGateLease, EmergencyGatePhase, GateLease, GateLeaseError, GateLeaseStore,
    StoredGateLease,
};

static UPGRADE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[doc = "Production root-owned immutable lease store under the fixed runtime layout."]
#[derive(Debug, Default, Clone, Copy)]
pub struct FileGateLeaseStore;

impl GateLeaseStore for FileGateLeaseStore {
    fn package_names(&mut self) -> Result<Vec<PackageName>, GateLeaseError> {
        super::discovery::package_names()
    }

    fn artifact_exists(&mut self, package: &PackageName) -> Result<bool, GateLeaseError> {
        let Some(root) = existing_gate_root()? else {
            return Ok(false);
        };
        Ok(path_present(&lease_path(&root, package))?
            || path_present(&tombstone_path(&root, package))?)
    }

    fn load(&mut self, package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        let Some(root) = existing_gate_root()? else {
            return Ok(None);
        };
        let Some((path, _tombstoned)) = select_artifact(&root, package)? else {
            return Ok(None);
        };
        let lease = StoredGateLease::parse(&read_secure_lease(&path)?)?;
        if lease.package_name() != package || lease.user_id() != crate::domain::UserId::PRIMARY {
            return Err(GateLeaseError::InvalidArtifact);
        }
        Ok(Some(lease))
    }

    fn persist_emergency(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        publish_new(lease.package_name(), &lease.bytes())
    }

    fn mark_emergency_held(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        let Some(StoredGateLease::Emergency(existing)) = self.load(lease.package_name())? else {
            return Err(GateLeaseError::InvalidArtifact);
        };
        if existing != *lease || existing.phase() != super::EmergencyGatePhase::Prepared {
            return Err(GateLeaseError::InvalidArtifact);
        }
        replace_atomically(lease.package_name(), &lease.held().bytes())
    }

    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        publish_new(lease.package_name(), &lease.bytes())
    }

    fn confirm_enrollment(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        let Some(StoredGateLease::Emergency(existing)) = self.load(lease.package_name())? else {
            return Err(GateLeaseError::InvalidArtifact);
        };
        if existing.user_id() != lease.user_id()
            || existing.snapshot() != lease.snapshot()
            || existing.phase() != EmergencyGatePhase::Held
        {
            return Err(GateLeaseError::InvalidArtifact);
        }
        replace_atomically(lease.package_name(), &lease.bytes())
    }

    fn retire(&mut self, expected: &GateLease) -> Result<(), GateLeaseError> {
        let package = expected.package_name();
        let root = existing_gate_root()?.ok_or(GateLeaseError::Unavailable)?;
        let Some((path, tombstoned)) = select_artifact(&root, package)? else {
            let retired = retired_path(&root, package);
            if !path_present(&retired)? {
                return Err(GateLeaseError::Unavailable);
            }
            let lease = StoredGateLease::parse(&read_secure_lease(&retired)?)?;
            return if lease == StoredGateLease::Enrolled(expected.clone()) {
                Ok(())
            } else {
                Err(GateLeaseError::InvalidArtifact)
            };
        };
        let lease = StoredGateLease::parse(&read_secure_lease(&path)?)?;
        if lease != StoredGateLease::Enrolled(expected.clone()) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        retirement::retire(
            &root,
            &lease_path(&root, package),
            &tombstone_path(&root, package),
            &retired_path(&root, package),
            tombstoned,
        )
    }
}

fn publish_new(package: &PackageName, bytes: &[u8]) -> Result<(), GateLeaseError> {
    let root = ensure_gate_root()?;
    if select_artifact(&root, package)?.is_some() {
        return Err(GateLeaseError::InvalidArtifact);
    }
    let path = lease_path(&root, package);
    write_new_synced(&path, bytes).map_err(|_| GateLeaseError::Unavailable)?;
    match read_secure_lease(&path) {
        Ok(content) if content.as_bytes() == bytes => Ok(()),
        Ok(_) => cleanup_unsafe_publication(&root, &path, GateLeaseError::InvalidArtifact),
        Err(error) => cleanup_unsafe_publication(&root, &path, error),
    }
}

fn replace_atomically(package: &PackageName, bytes: &[u8]) -> Result<(), GateLeaseError> {
    let root = ensure_gate_root()?;
    let Some((_current, false)) = select_artifact(&root, package)? else {
        return Err(GateLeaseError::InvalidArtifact);
    };
    let path = lease_path(&root, package);
    let temporary = upgrade_path(&root, package);
    write_new_synced(&temporary, bytes).map_err(|_| GateLeaseError::Unavailable)?;
    match read_secure_lease(&temporary) {
        Ok(content) if content.as_bytes() == bytes => {}
        Ok(_) => {
            let _cleanup = fs::remove_file(&temporary);
            return Err(GateLeaseError::InvalidArtifact);
        }
        Err(error) => {
            let _cleanup = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    let result = fs::rename(&temporary, &path)
        .and_then(|()| sync_directory(&root))
        .map_err(|_| GateLeaseError::Unavailable);
    if result.is_err() {
        let _cleanup = fs::remove_file(temporary);
    }
    result
}

fn select_artifact(
    root: &Path,
    package: &PackageName,
) -> Result<Option<(PathBuf, bool)>, GateLeaseError> {
    let lease = lease_path(root, package);
    let tombstone = tombstone_path(root, package);
    match (path_present(&lease)?, path_present(&tombstone)?) {
        (false, false) => Ok(None),
        (true, false) => Ok(Some((lease, false))),
        (false, true) => Ok(Some((tombstone, true))),
        (true, true) => Err(GateLeaseError::InvalidArtifact),
    }
}

fn cleanup_unsafe_publication(
    root: &Path,
    path: &Path,
    error: GateLeaseError,
) -> Result<(), GateLeaseError> {
    let _cleanup = fs::remove_file(path);
    let _sync = sync_directory(root);
    Err(error)
}

fn lease_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join(format!("{package}.gate"))
}

fn tombstone_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join(format!(".{package}.gate.retiring"))
}

fn retired_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join(format!(".{package}.gate.retired"))
}

fn upgrade_path(root: &Path, package: &PackageName) -> PathBuf {
    let sequence = UPGRADE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    root.join(format!(
        ".{package}.gate.upgrade-{}-{sequence}",
        std::process::id()
    ))
}
