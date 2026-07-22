use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::atomic_file::{ensure_directory, write_new_synced};
use crate::domain::{AppIdentity, DataInodes, PackageKey, PackageName, SlotId, UserId};

use super::manifest::encode;
use super::scan::load_package;
use super::{CatalogEntry, CatalogError, SecurityProfileProof};

#[doc = "Filesystem-backed create-only slot catalog store."]
#[derive(Debug, Clone)]
pub struct CatalogStore {
    root: PathBuf,
    packages: PathBuf,
}

impl CatalogStore {
    #[doc = "Creates or opens the root-only catalog directory."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let root = root.as_ref().to_path_buf();
        ensure_directory(&root)
            .map_err(|source| CatalogError::io("create catalog root", &root, source))?;
        let packages = root.join("packages");
        ensure_directory(&packages)
            .map_err(|source| CatalogError::io("create catalog packages", &packages, source))?;
        Ok(Self { root, packages })
    }

    #[doc = "Returns the catalog root used to derive immutable artifact paths."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[doc = "Creates one package catalog with its immutable Android-owned base entry."]
    pub fn create_base(
        &self,
        package_key: PackageKey,
        base_inodes: DataInodes,
        enrolled_identity: AppIdentity,
        security_profile: SecurityProfileProof,
    ) -> Result<CatalogEntry, CatalogError> {
        let slot_id = SlotId::base();
        let package_dir = self.package_dir(&package_key);
        match fs::symlink_metadata(&package_dir) {
            Ok(_) => {
                let path = self.manifest_path(&package_key, &slot_id);
                if path.is_file() {
                    return Err(duplicate(&package_key, slot_id));
                }
                return Err(CatalogError::Corrupt(format!(
                    "pre-existing package catalog {}",
                    package_dir.display()
                )));
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(CatalogError::io(
                    "inspect package catalog",
                    &package_dir,
                    source,
                ));
            }
        }
        let created_version_code = enrolled_identity.version_code();
        let entry = CatalogEntry::new(
            package_key,
            slot_id,
            base_inodes,
            enrolled_identity,
            created_version_code,
            security_profile,
        )?;
        self.publish(&entry)?;
        Ok(entry)
    }

    #[doc = "Appends one immutable non-base slot after validating its base anchor."]
    #[allow(
        clippy::too_many_arguments,
        reason = "all security-critical manifest fields are explicit"
    )]
    pub fn append_slot(
        &self,
        package_key: &PackageKey,
        slot_id: SlotId,
        inodes: DataInodes,
        enrolled_identity: &AppIdentity,
        created_version_code: u64,
        security_profile: SecurityProfileProof,
    ) -> Result<CatalogEntry, CatalogError> {
        if slot_id.is_base() {
            return Err(CatalogError::Invalid(
                "append requires a non-base slot".to_owned(),
            ));
        }
        let entries = self.list(package_key)?;
        let Some(base) = entries.iter().find(|entry| entry.slot_id().is_base()) else {
            return Err(CatalogError::MissingBase(
                package_key.package_name().clone(),
            ));
        };
        if entries.iter().any(|entry| entry.slot_id() == &slot_id) {
            return Err(duplicate(package_key, slot_id));
        }
        if base.enrolled_identity() != enrolled_identity {
            return Err(CatalogError::IdentityMismatch(
                package_key.package_name().clone(),
            ));
        }
        if shares_inodes(inodes, base.inodes()) {
            return Err(CatalogError::BaseInodeReuse {
                package: package_key.package_name().clone(),
                slot: slot_id,
            });
        }
        if entries
            .iter()
            .any(|entry| shares_inodes(inodes, entry.inodes()))
        {
            return Err(CatalogError::Invalid(
                "CE or DE inode is already assigned to another slot".to_owned(),
            ));
        }
        let entry = CatalogEntry::new(
            package_key.clone(),
            slot_id,
            inodes,
            enrolled_identity.clone(),
            created_version_code,
            security_profile,
        )?;
        self.publish(&entry)?;
        Ok(entry)
    }

    #[doc = "Lists and verifies every immutable slot for one package."]
    pub fn list(&self, package_key: &PackageKey) -> Result<Vec<CatalogEntry>, CatalogError> {
        let package_dir = self.package_dir(package_key);
        let metadata = match fs::symlink_metadata(&package_dir) {
            Ok(value) => value,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(CatalogError::io(
                    "inspect package catalog",
                    &package_dir,
                    source,
                ));
            }
        };
        if !metadata.file_type().is_dir() {
            return Err(CatalogError::Corrupt(
                "unexpected package catalog artifact".to_owned(),
            ));
        }
        load_package(&package_dir, package_key)
    }

    #[doc = "Looks up one slot only after verifying the complete package catalog."]
    pub fn lookup(
        &self,
        package_key: &PackageKey,
        slot_id: &SlotId,
    ) -> Result<Option<CatalogEntry>, CatalogError> {
        Ok(self
            .list(package_key)?
            .into_iter()
            .find(|entry| entry.slot_id() == slot_id))
    }

    #[doc = "Enumerates every package and slot while rejecting unknown root artifacts."]
    pub fn enumerate(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
        let mut packages = Vec::new();
        for entry in fs::read_dir(&self.packages)
            .map_err(|source| CatalogError::io("read catalog packages", &self.packages, source))?
        {
            let entry = entry.map_err(|source| {
                CatalogError::io("read package artifact", &self.packages, source)
            })?;
            let file_type = entry.file_type().map_err(|source| {
                CatalogError::io("inspect package artifact", &entry.path(), source)
            })?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| CatalogError::Corrupt("non-UTF-8 package artifact".to_owned()))?;
            let package = PackageName::parse(name).map_err(|_| {
                CatalogError::Corrupt(format!("unexpected package artifact {name}"))
            })?;
            if !file_type.is_dir() {
                return Err(CatalogError::Corrupt(format!(
                    "unexpected package artifact {name}"
                )));
            }
            packages.push(package);
        }
        packages.sort();
        let mut result = Vec::new();
        for package in packages {
            result.extend(self.list(&PackageKey::new(package, UserId::PRIMARY))?);
        }
        Ok(result)
    }

    fn publish(&self, entry: &CatalogEntry) -> Result<(), CatalogError> {
        let path = self.manifest_path(entry.package_key(), entry.slot_id());
        let bytes = encode(entry)?;
        write_new_synced(&path, &bytes).map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                duplicate(entry.package_key(), entry.slot_id().clone())
            } else {
                CatalogError::io("publish slot manifest", &path, source)
            }
        })
    }

    fn package_dir(&self, package_key: &PackageKey) -> PathBuf {
        self.packages.join(package_key.package_name().as_str())
    }

    fn manifest_path(&self, package_key: &PackageKey, slot_id: &SlotId) -> PathBuf {
        self.package_dir(package_key)
            .join("slots")
            .join(format!("{}.json", slot_id.as_str()))
    }
}

fn shares_inodes(left: DataInodes, right: DataInodes) -> bool {
    left.ce() == right.ce() || left.de() == right.de()
}

fn duplicate(package_key: &PackageKey, slot: SlotId) -> CatalogError {
    CatalogError::DuplicateSlot {
        package: package_key.package_name().clone(),
        slot,
    }
}
