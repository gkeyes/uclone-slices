use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{PackageName, SlotId};
use crate::store_security::{self, StoreSecurityError};

use super::{SlotDisplayName, SlotMetadata, SlotMetadataError, SlotRecordState, SlotSeedMode};

#[doc = "Filesystem-backed append-only slot metadata streams."]
#[derive(Debug, Clone)]
pub struct SlotMetadataStore {
    root: PathBuf,
    packages: PathBuf,
    owner_uid: u32,
}

impl SlotMetadataStore {
    #[doc = "Creates or opens the root-only slot metadata directory."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, SlotMetadataError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = store_security::initialize_root(&root, "slot metadata")
            .map_err(|error| map_security("initialize slot metadata", &root, error))?;
        let packages = root.join("packages");
        secure_directory(&packages, owner_uid)?;
        Ok(Self {
            root,
            packages,
            owner_uid,
        })
    }

    #[doc = "Publishes the creating revision for a new non-base slot."]
    pub fn create(
        &self,
        package: &PackageName,
        slot: SlotId,
        display_name: SlotDisplayName,
        seed_mode: SlotSeedMode,
        version_code: u64,
    ) -> Result<SlotMetadata, SlotMetadataError> {
        if self.latest(package, &slot)?.is_some() {
            return Err(SlotMetadataError::AlreadyExists);
        }
        let value =
            SlotMetadata::initial(package.clone(), slot, display_name, seed_mode, version_code)?;
        self.publish(&value)?;
        Ok(value)
    }

    #[doc = "Loads and verifies the newest revision for one exact slot."]
    pub fn latest(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<Option<SlotMetadata>, SlotMetadataError> {
        let revisions = self.revisions(package, slot);
        match fs::symlink_metadata(&revisions) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(SlotMetadataError::io(
                    "inspect slot metadata",
                    &revisions,
                    error,
                ));
            }
            Ok(_) => {}
        }
        let mut files = revision_files(&revisions)?;
        files.sort();
        if files.is_empty() || files.len() > 64 {
            return Err(SlotMetadataError::Corrupt(
                "invalid slot revision count".to_owned(),
            ));
        }
        let mut previous = None;
        for path in files {
            let bytes =
                store_security::read_record(&path, self.owner_uid, "slot metadata revision")
                    .map_err(|error| map_security("read slot metadata revision", &path, error))?;
            let value: SlotMetadata = serde_json::from_slice(&bytes)?;
            if value.package() != package || value.slot() != slot {
                return Err(SlotMetadataError::Corrupt(
                    "slot metadata path identity mismatch".to_owned(),
                ));
            }
            value.verify(previous.as_ref())?;
            previous = Some(value);
        }
        Ok(previous)
    }

    #[doc = "Lists the verified latest revision of every package slot."]
    pub fn list(&self, package: &PackageName) -> Result<Vec<SlotMetadata>, SlotMetadataError> {
        let slots = self.package(package).join("slots");
        match fs::symlink_metadata(&slots) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(SlotMetadataError::io(
                    "inspect package slots",
                    &slots,
                    error,
                ));
            }
            Ok(_) => {}
        }
        let mut result = Vec::new();
        for entry in fs::read_dir(&slots)
            .map_err(|error| SlotMetadataError::io("read package slots", &slots, error))?
        {
            let entry =
                entry.map_err(|error| SlotMetadataError::io("read slot entry", &slots, error))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| SlotMetadataError::Corrupt("non-UTF-8 slot entry".to_owned()))?;
            if !entry.file_type().is_ok_and(|value| value.is_dir()) {
                return Err(SlotMetadataError::Corrupt(format!(
                    "unexpected slot entry {name}"
                )));
            }
            let slot = SlotId::parse(name)
                .map_err(|_| SlotMetadataError::Corrupt(format!("invalid slot entry {name}")))?;
            result.push(
                self.latest(package, &slot)?
                    .ok_or(SlotMetadataError::NotFound)?,
            );
        }
        result.sort_by(|left, right| left.slot().cmp(right.slot()));
        Ok(result)
    }

    #[doc = "Appends a name, lifecycle, or last-opened revision."]
    pub fn update(
        &self,
        package: &PackageName,
        slot: &SlotId,
        display_name: SlotDisplayName,
        state: SlotRecordState,
        opened_version: u64,
    ) -> Result<SlotMetadata, SlotMetadataError> {
        let current = self
            .latest(package, slot)?
            .ok_or(SlotMetadataError::NotFound)?;
        let next = current.next(display_name, state, opened_version)?;
        self.publish(&next)?;
        Ok(next)
    }

    #[doc = "Returns the metadata persistence root."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn publish(&self, value: &SlotMetadata) -> Result<(), SlotMetadataError> {
        let package = self.package(value.package());
        secure_directory(&package, self.owner_uid)?;
        let slots = package.join("slots");
        secure_directory(&slots, self.owner_uid)?;
        let slot = slots.join(value.slot().as_str());
        secure_directory(&slot, self.owner_uid)?;
        let revisions = slot.join("revisions");
        secure_directory(&revisions, self.owner_uid)?;
        let path = revisions.join(format!("{:016}.json", value.generation()));
        let bytes = serde_json::to_vec(value)?;
        store_security::write_new_record(&path, &bytes, self.owner_uid, "slot metadata revision")
            .map_err(|error| map_security("publish slot metadata", &path, error))
    }

    fn package(&self, package: &PackageName) -> PathBuf {
        self.packages.join(package.as_str())
    }

    fn revisions(&self, package: &PackageName, slot: &SlotId) -> PathBuf {
        self.package(package)
            .join("slots")
            .join(slot.as_str())
            .join("revisions")
    }
}

fn revision_files(path: &Path) -> Result<Vec<PathBuf>, SlotMetadataError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(|error| SlotMetadataError::io("read slot metadata", path, error))?
    {
        let entry = entry
            .map_err(|error| SlotMetadataError::io("read slot metadata entry", path, error))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| SlotMetadataError::Corrupt("non-UTF-8 slot revision".to_owned()))?;
        if !valid_revision_name(name) || !entry.file_type().is_ok_and(|value| value.is_file()) {
            return Err(SlotMetadataError::Corrupt(format!(
                "unexpected slot metadata artifact {name}"
            )));
        }
        files.push(entry.path());
    }
    Ok(files)
}

fn secure_directory(path: &Path, owner_uid: u32) -> Result<(), SlotMetadataError> {
    store_security::ensure_child_directory(path, owner_uid, "slot metadata")
        .map_err(|error| map_security("create slot metadata directory", path, error))
}

fn valid_revision_name(name: &str) -> bool {
    name.strip_suffix(".json")
        .is_some_and(|prefix| name.len() == 21 && prefix.bytes().all(|byte| byte.is_ascii_digit()))
}

fn map_security(action: &'static str, path: &Path, error: StoreSecurityError) -> SlotMetadataError {
    match error {
        StoreSecurityError::Io(source) => SlotMetadataError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => SlotMetadataError::Corrupt(message),
    }
}
