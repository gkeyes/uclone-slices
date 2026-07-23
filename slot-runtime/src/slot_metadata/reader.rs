use std::fs;
use std::path::Path;

use crate::domain::{PackageName, SlotId};

use super::epoch::{EpochCheckpoint, EpochCommit};
use super::stream::{VerifiedEpoch, VerifiedStream};
use super::{SlotMetadata, SlotMetadataError, storage};

const MAX_LEGACY_REVISIONS: usize = 64;
const MAX_EPOCH_REVISIONS: usize = 48;

#[derive(Clone, Copy)]
struct RevisionRead<'a> {
    owner_uid: u32,
    package: &'a PackageName,
    slot: &'a SlotId,
    checkpoint: Option<&'a SlotMetadata>,
    limit: usize,
    allow_empty: bool,
}

pub(super) fn load(
    slot_path: &Path,
    owner_uid: u32,
    package: &PackageName,
    slot: &SlotId,
) -> Result<Option<VerifiedStream>, SlotMetadataError> {
    let revisions = slot_path.join("revisions");
    if !exists(&revisions)? {
        return if exists(slot_path)? {
            Err(corrupt("incomplete slot metadata stream"))
        } else {
            Ok(None)
        };
    }
    storage::validate_directory(slot_path, owner_uid)?;
    validate_names(slot_path, &["revisions", "epochs"])?;
    let legacy = load_revisions(
        &revisions,
        RevisionRead {
            owner_uid,
            package,
            slot,
            checkpoint: None,
            limit: MAX_LEGACY_REVISIONS,
            allow_empty: false,
        },
    )?;
    let legacy_count = legacy.len();
    let latest = legacy
        .last()
        .cloned()
        .ok_or_else(|| corrupt("empty legacy slot metadata stream"))?;
    let epochs = slot_path.join("epochs");
    if exists(&epochs)? {
        load_epochs(owner_uid, package, slot, &epochs, latest, legacy_count).map(Some)
    } else {
        Ok(Some(VerifiedStream {
            latest,
            legacy_count,
            current_epoch: None,
        }))
    }
}

fn load_epochs(
    owner_uid: u32,
    package: &PackageName,
    slot: &SlotId,
    epochs: &Path,
    mut latest: SlotMetadata,
    legacy_count: usize,
) -> Result<VerifiedStream, SlotMetadataError> {
    storage::validate_directory(epochs, owner_uid)?;
    let epoch_entries = storage::numbered_directories(epochs)?;
    if epoch_entries.is_empty() {
        return Err(corrupt("ambiguous slot metadata epoch head"));
    }
    let mut previous_epoch = None;
    let mut previous_count = legacy_count;
    let mut previous_commit_sha: Option<String> = None;
    let mut current = None;
    for (index, (epoch, epoch_path)) in epoch_entries.iter().enumerate() {
        let expected = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| corrupt("slot metadata epoch overflow"))?;
        if *epoch != expected {
            return Err(corrupt("non-contiguous slot metadata epochs"));
        }
        validate_names(epoch_path, &["checkpoint.json", "commit.json", "revisions"])?;
        let checkpoint: EpochCheckpoint =
            storage::read_json(&epoch_path.join("checkpoint.json"), owner_uid, "checkpoint")?;
        checkpoint.verify(expected, previous_epoch, previous_count, &latest)?;
        let commit: EpochCommit =
            storage::read_json(&epoch_path.join("commit.json"), owner_uid, "epoch commit")?;
        commit.verify(
            expected,
            checkpoint.sha256(),
            previous_commit_sha.as_deref(),
        )?;
        let revisions = load_revisions(
            &epoch_path.join("revisions"),
            RevisionRead {
                owner_uid,
                package,
                slot,
                checkpoint: Some(checkpoint.canonical_record()),
                limit: MAX_EPOCH_REVISIONS,
                allow_empty: true,
            },
        )?;
        if let Some(value) = revisions.last() {
            latest = value.clone();
        }
        previous_epoch = Some(expected);
        previous_count = revisions.len();
        previous_commit_sha = Some(commit.sha256().to_owned());
        current = Some(VerifiedEpoch {
            number: expected,
            revision_count: revisions.len(),
            commit_sha256: commit.sha256().to_owned(),
        });
    }
    Ok(VerifiedStream {
        latest,
        legacy_count,
        current_epoch: current,
    })
}

fn load_revisions(
    path: &Path,
    read: RevisionRead<'_>,
) -> Result<Vec<SlotMetadata>, SlotMetadataError> {
    storage::validate_directory(path, read.owner_uid)?;
    let files = storage::revision_files(path)?;
    if files.len() > read.limit || (!read.allow_empty && files.is_empty()) {
        return Err(corrupt("invalid slot metadata revision count"));
    }
    let mut previous = read.checkpoint.cloned();
    let mut values = Vec::with_capacity(files.len());
    for path in files {
        let value: SlotMetadata =
            storage::read_json(&path, read.owner_uid, "slot metadata revision")?;
        if value.package() != read.package || value.slot() != read.slot {
            return Err(corrupt("slot metadata path identity mismatch"));
        }
        value.verify(previous.as_ref())?;
        previous = Some(value.clone());
        values.push(value);
    }
    Ok(values)
}

fn validate_names(path: &Path, allowed: &[&str]) -> Result<(), SlotMetadataError> {
    for entry in fs::read_dir(path)
        .map_err(|error| SlotMetadataError::io("read slot metadata directory", path, error))?
    {
        let entry = entry
            .map_err(|error| SlotMetadataError::io("read slot metadata entry", path, error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| corrupt("non-UTF-8 slot metadata artifact"))?;
        if !allowed.contains(&name.as_str()) {
            return Err(corrupt(&format!(
                "unexpected slot metadata artifact {name}"
            )));
        }
    }
    Ok(())
}

fn exists(path: &Path) -> Result<bool, SlotMetadataError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(SlotMetadataError::io("inspect slot metadata", path, error)),
    }
}

fn corrupt(message: &str) -> SlotMetadataError {
    SlotMetadataError::Corrupt(message.to_owned())
}
