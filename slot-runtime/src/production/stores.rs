use std::fs::{self, File};
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::catalog::CatalogStore;
use crate::domain::{PackageKey, PackageName};
use crate::enrollment::EnrollmentStore;
use crate::enrollment_attempt::EnrollmentAttemptStore;
use crate::journal::JournalStore;
use crate::layout::RuntimeLayout;
use crate::package_state::{PackageStateRevision, PackageStateStore};
use crate::registry::RegistryStore;
use crate::service::ServiceError;
use crate::slot_metadata::SlotMetadataStore;

const MAX_ARTIFACT_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone)]
pub(super) struct ProductionStores {
    pub(super) enrollment: EnrollmentStore,
    pub(super) attempts: EnrollmentAttemptStore,
    pub(super) catalog: CatalogStore,
    pub(super) package_state: PackageStateStore,
    pub(super) journal: JournalStore,
    pub(super) registry: RegistryStore,
    pub(super) slot_metadata: SlotMetadataStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PublishedDigests {
    pub(super) enrollment: String,
    pub(super) base_catalog: String,
    pub(super) package_state: String,
}

impl ProductionStores {
    pub(super) fn open_fixed() -> Result<Self, ServiceError> {
        Ok(Self {
            enrollment: EnrollmentStore::new(RuntimeLayout::enrollment_root())
                .map_err(|_| ServiceError::Internal)?,
            attempts: EnrollmentAttemptStore::fixed().map_err(|_| ServiceError::Internal)?,
            catalog: CatalogStore::new(RuntimeLayout::catalog_root())
                .map_err(|_| ServiceError::Internal)?,
            package_state: PackageStateStore::new(RuntimeLayout::package_state_root())
                .map_err(|_| ServiceError::Internal)?,
            journal: JournalStore::new(RuntimeLayout::journal_root())
                .map_err(|_| ServiceError::Internal)?,
            registry: RegistryStore::new(RuntimeLayout::registry_root())
                .map_err(|_| ServiceError::Internal)?,
            slot_metadata: SlotMetadataStore::new(RuntimeLayout::slot_metadata_root())
                .map_err(|_| ServiceError::Internal)?,
        })
    }

    pub(super) fn published_digests(
        &self,
        key: &PackageKey,
        state: &PackageStateRevision,
    ) -> Result<PublishedDigests, ServiceError> {
        let package = key.package_name();
        Ok(PublishedDigests {
            enrollment: hash_file(&enrollment_path(package))?,
            base_catalog: hash_file(&base_catalog_path(package))?,
            package_state: hash_file(
                &self
                    .package_state
                    .revision_path(package, state.generation()),
            )?,
        })
    }
}

fn enrollment_path(package: &PackageName) -> PathBuf {
    RuntimeLayout::enrollment_root()
        .join("packages")
        .join(package.as_str())
        .join("enrollment.json")
}

fn base_catalog_path(package: &PackageName) -> PathBuf {
    RuntimeLayout::catalog_root()
        .join("packages")
        .join(package.as_str())
        .join("slots")
        .join("base.json")
}

fn hash_file(path: &Path) -> Result<String, ServiceError> {
    let before = fs::symlink_metadata(path).map_err(|_| ServiceError::RecoveryRequired)?;
    if !trusted_artifact(&before) || before.len() > MAX_ARTIFACT_BYTES {
        return Err(ServiceError::RecoveryRequired);
    }
    let file = File::open(path).map_err(|_| ServiceError::RecoveryRequired)?;
    let opened = file
        .metadata()
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if !same_artifact(&before, &opened) {
        return Err(ServiceError::RecoveryRequired);
    }
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if u64::try_from(bytes.len()).map_err(|_| ServiceError::RecoveryRequired)? > MAX_ARTIFACT_BYTES
    {
        return Err(ServiceError::RecoveryRequired);
    }
    let after = fs::symlink_metadata(path).map_err(|_| ServiceError::RecoveryRequired)?;
    if !same_artifact(&before, &after) || after.len() != opened.len() {
        return Err(ServiceError::RecoveryRequired);
    }
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn trusted_artifact(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && metadata.uid() == 0
        && metadata.nlink() == 1
        && metadata.mode() & 0o777 == 0o600
}

fn same_artifact(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    trusted_artifact(right)
        && left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
}
