use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;

use crate::atomic_file::{ensure_directory, sync_directory};
use crate::domain::{GateSnapshot, PackageKey, UserId};

use super::super::super::EnrollmentAttemptError;
use super::super::super::model::{EnrollmentAttempt, EnrollmentAttemptPhase};
use super::super::super::storage;
use super::super::EnrollmentAttemptStore;

impl EnrollmentAttemptStore {
    #[doc = "Creates the first pending generation without replacing an existing attempt."]
    pub fn create_pending(
        &self,
        package: &PackageKey,
        gate_snapshot: GateSnapshot,
    ) -> Result<EnrollmentAttempt, EnrollmentAttemptError> {
        super::super::validate_key(package)?;
        let package_dir = self.package_path(package);
        match fs::symlink_metadata(&package_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                return Err(EnrollmentAttemptError::AlreadyExists(package.clone()));
            }
            Ok(_) => {
                return Err(EnrollmentAttemptError::Corrupt(
                    "attempt package is not a directory".to_owned(),
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(EnrollmentAttemptError::io(
                    "inspect enrollment attempt",
                    &package_dir,
                    source,
                ));
            }
        }
        fs::create_dir(&package_dir).map_err(|source| {
            EnrollmentAttemptError::io("create enrollment attempt", &package_dir, source)
        })?;
        fs::set_permissions(&package_dir, fs::Permissions::from_mode(0o700)).map_err(|source| {
            EnrollmentAttemptError::io("protect enrollment attempt", &package_dir, source)
        })?;
        let generations = package_dir.join(storage::GENERATIONS);
        ensure_directory(&generations).map_err(|source| {
            EnrollmentAttemptError::io("create enrollment generations", &generations, source)
        })?;
        let attempt = EnrollmentAttempt::new(
            package.clone(),
            gate_snapshot,
            EnrollmentAttemptPhase::Pending,
            None,
            1,
            None,
        )?;
        storage::write_generation(&package_dir, &attempt)?;
        sync_directory(&self.attempts).map_err(|source| {
            EnrollmentAttemptError::io("sync enrollment attempts", &self.attempts, source)
        })?;
        Ok(attempt.with_authority(true))
    }

    #[doc = "Loads and verifies one attempt; markerless commits are recovery-required."]
    pub fn load(
        &self,
        package: &PackageKey,
    ) -> Result<Option<EnrollmentAttempt>, EnrollmentAttemptError> {
        super::super::validate_key(package)?;
        let package_dir = self.package_path(package);
        match fs::symlink_metadata(&package_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(EnrollmentAttemptError::Corrupt(
                    "attempt package is not a directory".to_owned(),
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(EnrollmentAttemptError::io(
                    "inspect enrollment attempt",
                    &package_dir,
                    source,
                ));
            }
        }
        storage::validate_package_directory(&package_dir)?;
        let mut records = storage::load_generations(&package_dir, package)?;
        let latest = records.pop().ok_or_else(|| {
            EnrollmentAttemptError::Corrupt("missing latest attempt generation".to_owned())
        })?;
        if let Some(marker) = storage::load_marker(&package_dir)? {
            if latest.stored_phase() != EnrollmentAttemptPhase::Committed {
                return Err(EnrollmentAttemptError::Corrupt(
                    "commit marker exists without committed generation".to_owned(),
                ));
            }
            marker.verify(package, &latest)?;
            Ok(Some(latest.with_authority(true)))
        } else {
            let authoritative = latest.stored_phase() != EnrollmentAttemptPhase::Committed;
            Ok(Some(latest.with_authority(authoritative)))
        }
    }

    #[doc = "Enumerates every allowlisted attempt, independent of `EnrollmentStore`."]
    pub fn list(&self) -> Result<Vec<EnrollmentAttempt>, EnrollmentAttemptError> {
        let entries = fs::read_dir(&self.attempts).map_err(|source| {
            EnrollmentAttemptError::io("read enrollment attempts", &self.attempts, source)
        })?;
        let mut package = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| {
                EnrollmentAttemptError::io("read enrollment attempt entry", &self.attempts, source)
            })?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                EnrollmentAttemptError::Corrupt("non-UTF-8 attempt package".to_owned())
            })?;
            let key = crate::domain::PackageName::parse(name).map_err(|_| {
                EnrollmentAttemptError::Corrupt(format!("unexpected attempt package {name}"))
            })?;
            if !entry
                    .file_type()
                    .map_err(|source| {
                        EnrollmentAttemptError::io("inspect attempt package", &entry.path(), source)
                    })?
                    .is_dir()
            {
                return Err(EnrollmentAttemptError::Corrupt(format!(
                    "unexpected attempt package {name}"
                )));
            }
            package.push(PackageKey::new(key, UserId::PRIMARY));
        }
        package.sort_by(|left, right| left.package_name().cmp(right.package_name()));
        package
            .into_iter()
            .map(|key| {
                self.load(&key)
                    .and_then(|value| value.ok_or(EnrollmentAttemptError::NotFound(key)))
            })
            .collect()
    }
}
