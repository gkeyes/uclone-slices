use std::fs;
use std::path::{Path, PathBuf};

use super::{
    SlotDisplayName, SlotMetadata, SlotMetadataError, SlotRecordState, SlotSeedMode, storage,
    stream,
};
use crate::domain::{PackageName, SlotId};

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
        let owner_uid = storage::initialize_root(&root)?;
        let packages = root.join("packages");
        storage::secure_directory(&packages, owner_uid)?;
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
        Ok(
            stream::load(&self.slot(package, slot), self.owner_uid, package, slot)?
                .map(|value| value.latest().clone()),
        )
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
        let slot_path = self.slot(package, slot);
        let stream = stream::load(&slot_path, self.owner_uid, package, slot)?
            .ok_or(SlotMetadataError::NotFound)?;
        let next = stream.latest().next(display_name, state, opened_version)?;
        stream::publish_update(&slot_path, self.owner_uid, &stream, &next)?;
        Ok(next)
    }

    #[doc = "Returns the metadata persistence root."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn publish(&self, value: &SlotMetadata) -> Result<(), SlotMetadataError> {
        let package = self.package(value.package());
        storage::secure_directory(&package, self.owner_uid)?;
        let slots = package.join("slots");
        storage::secure_directory(&slots, self.owner_uid)?;
        stream::publish_legacy(&slots.join(value.slot().as_str()), self.owner_uid, value)
    }

    fn package(&self, package: &PackageName) -> PathBuf {
        self.packages.join(package.as_str())
    }

    fn slot(&self, package: &PackageName, slot: &SlotId) -> PathBuf {
        self.package(package).join("slots").join(slot.as_str())
    }
}
