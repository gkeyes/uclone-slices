use std::fs;
use std::path::{Path, PathBuf};

use super::storage;
use super::{PackageStateError, PackageStateReason, PackageStateRevision};
use crate::domain::{PackageKey, PackageName, UserId};
use crate::lifecycle::LifecycleState;

#[doc = "Filesystem-backed append-only package lifecycle-state store."]
#[derive(Debug, Clone)]
pub struct PackageStateStore {
    root: PathBuf,
    packages: PathBuf,
    owner_uid: u32,
}

impl PackageStateStore {
    #[doc = "Creates or opens a root-only package lifecycle-state directory."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PackageStateError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = storage::initialize_root(&root)?;
        let packages = root.join("packages");
        storage::ensure_directory(&packages, owner_uid)?;
        let store = Self {
            root,
            packages,
            owner_uid,
        };
        store.validate_root()?;
        Ok(store)
    }

    #[doc = "Initializes one user-zero package stream in the normal state."]
    pub fn initialize(
        &self,
        package: &PackageKey,
    ) -> Result<PackageStateRevision, PackageStateError> {
        require_primary(package)?;
        self.validate_root()?;
        let revisions = self.revisions_path(package.package_name());
        if storage::load_all(&revisions, package.package_name(), self.owner_uid)?.is_some() {
            return Err(PackageStateError::AlreadyExists(package.clone()));
        }
        let package_directory = self.package_path(package.package_name());
        storage::ensure_directory(&package_directory, self.owner_uid)?;
        storage::ensure_directory(&revisions, self.owner_uid)?;
        let revision = PackageStateRevision::initialized(package)?;
        storage::write_revision(&revisions, self.owner_uid, &revision)?;
        storage::sync_directory(&package_directory, self.owner_uid)?;
        Ok(revision)
    }

    #[doc = "Loads and verifies the newest revision for one user-zero package."]
    pub fn latest(
        &self,
        package: &PackageKey,
    ) -> Result<Option<PackageStateRevision>, PackageStateError> {
        require_primary(package)?;
        self.validate_root()?;
        Ok(storage::load_all(
            &self.revisions_path(package.package_name()),
            package.package_name(),
            self.owner_uid,
        )?
        .and_then(|mut revisions| revisions.pop()))
    }

    #[doc = "Appends a legal state transition after the caller's expected state."]
    pub fn transition(
        &self,
        package: &PackageKey,
        expected_previous: LifecycleState,
        next: LifecycleState,
        reason: PackageStateReason,
    ) -> Result<PackageStateRevision, PackageStateError> {
        require_primary(package)?;
        self.validate_root()?;
        let revisions = self.revisions_path(package.package_name());
        let previous = storage::load_all(&revisions, package.package_name(), self.owner_uid)?
            .and_then(|mut revisions| revisions.pop())
            .ok_or_else(|| PackageStateError::NotInitialized(package.clone()))?;
        if previous.package_key() != *package {
            return Err(PackageStateError::Corrupt(
                "package state directory does not match package key".to_owned(),
            ));
        }
        if previous.lifecycle_state() != expected_previous {
            return Err(PackageStateError::UnexpectedPrevious {
                expected: expected_previous,
                actual: previous.lifecycle_state(),
            });
        }
        if !expected_previous.can_transition_to(next) {
            return Err(PackageStateError::IllegalTransition {
                previous: expected_previous,
                next,
            });
        }
        let revision = PackageStateRevision::transitioned(&previous, next, reason)?;
        storage::write_revision(&revisions, self.owner_uid, &revision)?;
        Ok(revision)
    }

    #[doc = "Enumerates attributable package entries in lexical order."]
    #[doc = "Call `latest` for each returned package to validate its isolated stream."]
    pub fn enumerate_packages(&self) -> Result<Vec<PackageName>, PackageStateError> {
        self.validate_root()?;
        let entries = fs::read_dir(&self.packages).map_err(|source| {
            PackageStateError::io("enumerate package state packages", &self.packages, source)
        })?;
        let mut packages = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| {
                PackageStateError::io("read package state entry", &self.packages, source)
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(PackageStateError::Corrupt(
                    "non-UTF-8 package entry".to_owned(),
                ));
            };
            let package = PackageName::parse(name).map_err(|_| {
                PackageStateError::Corrupt(format!("unexpected package state artifact {name}"))
            })?;
            packages.push(package);
        }
        packages.sort();
        Ok(packages)
    }

    #[doc = "Returns the package lifecycle-state store root."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[doc = "Returns a published revision path for test and diagnostic tooling."]
    pub fn revision_path(&self, package: &PackageName, generation: u64) -> PathBuf {
        self.revisions_path(package)
            .join(storage::revision_file_name(generation))
    }

    fn validate_root(&self) -> Result<(), PackageStateError> {
        storage::validate_directory(&self.root, self.owner_uid)?;
        let entries = fs::read_dir(&self.root).map_err(|source| {
            PackageStateError::io("read package state root", &self.root, source)
        })?;
        let mut found_packages = false;
        for entry in entries {
            let entry = entry.map_err(|source| {
                PackageStateError::io("read package state root entry", &self.root, source)
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(PackageStateError::Corrupt(
                    "non-UTF-8 package state root entry".to_owned(),
                ));
            };
            let file_type = entry.file_type().map_err(|source| {
                PackageStateError::io("inspect package state root entry", &entry.path(), source)
            })?;
            if name != "packages" || !file_type.is_dir() || found_packages {
                return Err(PackageStateError::Corrupt(format!(
                    "unexpected package state artifact {name}"
                )));
            }
            storage::validate_directory(&entry.path(), self.owner_uid)?;
            found_packages = true;
        }
        if !found_packages {
            return Err(PackageStateError::Corrupt(
                "package state root is missing packages".to_owned(),
            ));
        }
        Ok(())
    }

    fn revisions_path(&self, package: &PackageName) -> PathBuf {
        self.packages.join(package.as_str()).join("revisions")
    }

    fn package_path(&self, package: &PackageName) -> PathBuf {
        self.packages.join(package.as_str())
    }
}

fn require_primary(package: &PackageKey) -> Result<(), PackageStateError> {
    if package.user_id() != UserId::PRIMARY {
        return Err(PackageStateError::Corrupt(
            "package lifecycle state supports user 0 only".to_owned(),
        ));
    }
    Ok(())
}
