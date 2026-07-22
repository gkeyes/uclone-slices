use std::fs::{self, File};
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::catalog::CatalogStore;
use crate::compatibility_policy::CompatibilityPolicyStore;
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
    pub(super) compatibility_policy: CompatibilityPolicyStore,
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
    pub(super) compatibility_policy: String,
    pub(super) base_catalog: String,
    pub(super) package_state: String,
}

impl ProductionStores {
    pub(super) fn open_fixed() -> Result<Self, ServiceError> {
        Ok(Self {
            enrollment: EnrollmentStore::new(RuntimeLayout::enrollment_root())
                .map_err(|_| ServiceError::Internal)?,
            compatibility_policy: CompatibilityPolicyStore::new(
                RuntimeLayout::compatibility_policy_root(),
            )
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
        let owner_uid = self.trusted_store_owner()?;
        Ok(PublishedDigests {
            enrollment: hash_file(&enrollment_path(self.enrollment.root(), package), owner_uid)?,
            compatibility_policy: hash_file(
                &compatibility_policy_path(self.compatibility_policy.root(), package),
                owner_uid,
            )?,
            base_catalog: hash_file(&base_catalog_path(self.catalog.root(), package), owner_uid)?,
            package_state: hash_file(
                &self
                    .package_state
                    .revision_path(package, state.generation()),
                owner_uid,
            )?,
        })
    }

    fn trusted_store_owner(&self) -> Result<u32, ServiceError> {
        let roots = [
            self.enrollment.root(),
            self.compatibility_policy.root(),
            self.catalog.root(),
            self.package_state.root(),
        ];
        let first = fs::symlink_metadata(roots[0]).map_err(|_| ServiceError::RecoveryRequired)?;
        if !trusted_root(&first) {
            return Err(ServiceError::RecoveryRequired);
        }
        let owner_uid = first.uid();
        for root in roots.iter().skip(1) {
            let metadata =
                fs::symlink_metadata(root).map_err(|_| ServiceError::RecoveryRequired)?;
            if !trusted_root(&metadata) || metadata.uid() != owner_uid {
                return Err(ServiceError::RecoveryRequired);
            }
        }
        Ok(owner_uid)
    }
}

fn compatibility_policy_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join("packages")
        .join(package.as_str())
        .join("policy.json")
}

fn enrollment_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join("packages")
        .join(package.as_str())
        .join("enrollment.json")
}

fn base_catalog_path(root: &Path, package: &PackageName) -> PathBuf {
    root.join("packages")
        .join(package.as_str())
        .join("slots")
        .join("base.json")
}

fn hash_file(path: &Path, owner_uid: u32) -> Result<String, ServiceError> {
    let before = fs::symlink_metadata(path).map_err(|_| ServiceError::RecoveryRequired)?;
    if !trusted_artifact(&before, owner_uid) || before.len() > MAX_ARTIFACT_BYTES {
        return Err(ServiceError::RecoveryRequired);
    }
    let file = File::open(path).map_err(|_| ServiceError::RecoveryRequired)?;
    let opened = file
        .metadata()
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if !same_artifact(&before, &opened, owner_uid) {
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
    if !same_artifact(&before, &after, owner_uid) || after.len() != opened.len() {
        return Err(ServiceError::RecoveryRequired);
    }
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn trusted_root(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir() && metadata.mode() & 0o777 == 0o700
}

fn trusted_artifact(metadata: &fs::Metadata, owner_uid: u32) -> bool {
    metadata.file_type().is_file()
        && metadata.uid() == owner_uid
        && metadata.nlink() == 1
        && metadata.mode() & 0o777 == 0o600
}

fn same_artifact(left: &fs::Metadata, right: &fs::Metadata, owner_uid: u32) -> bool {
    trusted_artifact(right, owner_uid)
        && left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
}
