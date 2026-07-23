use std::path::Path;

use crate::domain::{PackageName, SlotId};

use super::epoch::{EpochCheckpoint, EpochCommit};
use super::{SlotMetadata, SlotMetadataError, reader, storage};

const MAX_EPOCH_REVISIONS: usize = 48;

#[derive(Debug)]
pub(super) struct VerifiedStream {
    pub(super) latest: SlotMetadata,
    pub(super) legacy_count: usize,
    pub(super) current_epoch: Option<VerifiedEpoch>,
}

#[derive(Debug)]
pub(super) struct VerifiedEpoch {
    pub(super) number: u64,
    pub(super) revision_count: usize,
    pub(super) commit_sha256: String,
}

impl VerifiedStream {
    pub(super) const fn latest(&self) -> &SlotMetadata {
        &self.latest
    }
}

pub(super) fn load(
    slot_path: &Path,
    owner_uid: u32,
    package: &PackageName,
    slot: &SlotId,
) -> Result<Option<VerifiedStream>, SlotMetadataError> {
    reader::load(slot_path, owner_uid, package, slot)
}

pub(super) fn publish_legacy(
    slot_path: &Path,
    owner_uid: u32,
    value: &SlotMetadata,
) -> Result<(), SlotMetadataError> {
    let slots = slot_path
        .parent()
        .ok_or_else(|| corrupt("invalid slot metadata path"))?;
    storage::ensure_synced_directory(slot_path, slots, owner_uid)?;
    let revisions = slot_path.join("revisions");
    storage::ensure_synced_directory(&revisions, slot_path, owner_uid)?;
    publish_revision(&revisions, owner_uid, value)
}

pub(super) fn publish_update(
    slot_path: &Path,
    owner_uid: u32,
    stream: &VerifiedStream,
    value: &SlotMetadata,
) -> Result<(), SlotMetadataError> {
    let epoch = match &stream.current_epoch {
        None => publish_epoch(
            slot_path,
            owner_uid,
            1,
            None,
            stream.legacy_count,
            stream.latest.clone(),
            None,
        )?,
        Some(current) if current.revision_count >= MAX_EPOCH_REVISIONS => publish_epoch(
            slot_path,
            owner_uid,
            current
                .number
                .checked_add(1)
                .ok_or_else(|| corrupt("slot metadata epoch overflow"))?,
            Some(current.number),
            current.revision_count,
            stream.latest.clone(),
            Some(current.commit_sha256.clone()),
        )?,
        Some(current) => current.number,
    };
    publish_revision(
        &slot_path
            .join("epochs")
            .join(format!("{epoch:016}"))
            .join("revisions"),
        owner_uid,
        value,
    )
}

fn publish_epoch(
    slot_path: &Path,
    owner_uid: u32,
    epoch: u64,
    previous_epoch: Option<u64>,
    previous_count: usize,
    canonical: SlotMetadata,
    previous_commit_sha: Option<String>,
) -> Result<u64, SlotMetadataError> {
    let epochs = slot_path.join("epochs");
    storage::ensure_synced_directory(&epochs, slot_path, owner_uid)?;
    let checkpoint = EpochCheckpoint::new(epoch, previous_epoch, previous_count, canonical)?;
    let commit = EpochCommit::new(epoch, checkpoint.sha256().to_owned(), previous_commit_sha)?;
    storage::publish_epoch_directory(&epochs, epoch, &checkpoint, &commit, owner_uid)?;
    Ok(epoch)
}

fn publish_revision(
    revisions: &Path,
    owner_uid: u32,
    value: &SlotMetadata,
) -> Result<(), SlotMetadataError> {
    storage::write_json(
        &revisions.join(format!("{:016}.json", value.generation())),
        value,
        owner_uid,
        "slot metadata revision",
    )
}

fn corrupt(message: &str) -> SlotMetadataError {
    SlotMetadataError::Corrupt(message.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "isolated epoch publication fixture aborts the invoking test"
    )]

    use std::fs;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    use tempfile::TempDir;

    use super::*;
    use crate::slot_metadata::{SlotDisplayName, SlotMetadataStore, SlotSeedMode};

    #[test]
    fn renamed_epoch_is_complete_and_readable_before_its_first_revision() {
        let root = TempDir::new().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let metadata_root = root.path().join("metadata");
        let store = SlotMetadataStore::new(&metadata_root).unwrap();
        let package = PackageName::parse("com.example.atomic").unwrap();
        let slot = SlotId::parse("slot-atomic").unwrap();
        let canonical = store
            .create(
                &package,
                slot.clone(),
                SlotDisplayName::parse("Atomic").unwrap(),
                SlotSeedMode::Blank,
                1,
            )
            .unwrap();
        let slot_path = metadata_root.join("packages/com.example.atomic/slots/slot-atomic");
        let owner_uid = fs::symlink_metadata(&metadata_root).unwrap().uid();

        publish_epoch(&slot_path, owner_uid, 1, None, 1, canonical.clone(), None).unwrap();

        assert_eq!(store.latest(&package, &slot).unwrap(), Some(canonical));
        let epoch = slot_path.join("epochs/0000000000000001");
        assert!(epoch.join("checkpoint.json").is_file());
        assert!(epoch.join("commit.json").is_file());
        assert_eq!(fs::read_dir(epoch.join("revisions")).unwrap().count(), 0);
    }
}
