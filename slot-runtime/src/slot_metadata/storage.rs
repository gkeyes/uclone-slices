use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::store_security::{self, StoreSecurityError};

use super::SlotMetadataError;

const DIRECTORY_MODE: u32 = 0o700;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn initialize_root(path: &Path) -> Result<u32, SlotMetadataError> {
    store_security::initialize_root(path, "slot metadata")
        .map_err(|error| map_security("initialize slot metadata", path, error))
}

pub(super) fn secure_directory(path: &Path, owner_uid: u32) -> Result<(), SlotMetadataError> {
    store_security::ensure_child_directory(path, owner_uid, "slot metadata")
        .map_err(|error| map_security("create slot metadata directory", path, error))
}

pub(super) fn validate_directory(path: &Path, owner_uid: u32) -> Result<(), SlotMetadataError> {
    store_security::validate_directory(path, owner_uid, "slot metadata")
        .map_err(|error| map_security("validate slot metadata directory", path, error))
}

pub(super) fn ensure_synced_directory(
    path: &Path,
    parent: &Path,
    owner_uid: u32,
) -> Result<(), SlotMetadataError> {
    secure_directory(path, owner_uid)?;
    sync_directory(parent, owner_uid)
}

pub(super) fn sync_directory(path: &Path, owner_uid: u32) -> Result<(), SlotMetadataError> {
    store_security::sync_directory(path, owner_uid, "slot metadata")
        .map_err(|error| map_security("sync slot metadata directory", path, error))
}

pub(super) fn read_json<T: DeserializeOwned>(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<T, SlotMetadataError> {
    let bytes = store_security::read_record(path, owner_uid, kind)
        .map_err(|error| map_security("read slot metadata record", path, error))?;
    serde_json::from_slice(&bytes).map_err(Into::into)
}

pub(super) fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
    owner_uid: u32,
    kind: &str,
) -> Result<(), SlotMetadataError> {
    let bytes = serde_json::to_vec(value)?;
    store_security::write_new_record(path, &bytes, owner_uid, kind)
        .map_err(|error| map_security("publish slot metadata record", path, error))
}

pub(super) fn revision_files(path: &Path) -> Result<Vec<PathBuf>, SlotMetadataError> {
    let mut files = Vec::new();
    for entry in read_directory(path)? {
        let name = entry_name(&entry)?;
        if !valid_record_name(&name) || !entry.file_type().is_ok_and(|value| value.is_file()) {
            return Err(corrupt(&format!(
                "unexpected slot metadata revision {name}"
            )));
        }
        files.push(entry.path());
    }
    files.sort();
    Ok(files)
}

pub(super) fn numbered_directories(path: &Path) -> Result<Vec<(u64, PathBuf)>, SlotMetadataError> {
    let mut directories = Vec::new();
    for entry in read_directory(path)? {
        let name = entry_name(&entry)?;
        let number = parse_numbered_name(&name)?;
        if !entry.file_type().is_ok_and(|value| value.is_dir()) {
            return Err(corrupt(&format!(
                "unexpected slot metadata epoch artifact {name}"
            )));
        }
        directories.push((number, entry.path()));
    }
    directories.sort_by_key(|(number, _)| *number);
    Ok(directories)
}

pub(super) fn publish_epoch_directory(
    epochs: &Path,
    epoch: u64,
    checkpoint: &impl Serialize,
    commit: &impl Serialize,
    owner_uid: u32,
) -> Result<PathBuf, SlotMetadataError> {
    let name = format!("{epoch:016}");
    let final_path = epochs.join(&name);
    reject_existing(&final_path, "slot metadata epoch")?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = epochs.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    fs::create_dir(&temporary)
        .map_err(|error| SlotMetadataError::io("create slot metadata epoch", &temporary, error))?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(DIRECTORY_MODE))
        .map_err(|error| SlotMetadataError::io("secure slot metadata epoch", &temporary, error))?;
    validate_directory(&temporary, owner_uid)?;
    let revisions = temporary.join("revisions");
    ensure_synced_directory(&revisions, &temporary, owner_uid)?;
    write_json(
        &temporary.join("checkpoint.json"),
        checkpoint,
        owner_uid,
        "slot metadata checkpoint",
    )?;
    write_json(
        &temporary.join("commit.json"),
        commit,
        owner_uid,
        "slot metadata epoch commit",
    )?;
    sync_directory(&revisions, owner_uid)?;
    sync_directory(&temporary, owner_uid)?;
    fs::rename(&temporary, &final_path)
        .map_err(|error| SlotMetadataError::io("commit slot metadata epoch", &final_path, error))?;
    sync_directory(epochs, owner_uid)?;
    validate_directory(&final_path, owner_uid)?;
    Ok(final_path)
}

pub(super) fn reject_existing(path: &Path, kind: &str) -> Result<(), SlotMetadataError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(corrupt(&format!("{kind} already exists"))),
        Err(error) => Err(SlotMetadataError::io("inspect slot metadata", path, error)),
    }
}

fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, SlotMetadataError> {
    fs::read_dir(path)
        .map_err(|error| SlotMetadataError::io("read slot metadata directory", path, error))?
        .map(|entry| {
            entry.map_err(|error| SlotMetadataError::io("read slot metadata entry", path, error))
        })
        .collect()
}

fn entry_name(entry: &fs::DirEntry) -> Result<String, SlotMetadataError> {
    entry
        .file_name()
        .into_string()
        .map_err(|_| corrupt("non-UTF-8 slot metadata artifact"))
}

fn parse_numbered_name(name: &str) -> Result<u64, SlotMetadataError> {
    let digits = name;
    if digits.len() != 16 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(corrupt(&format!(
            "unexpected slot metadata artifact {name}"
        )));
    }
    digits
        .parse()
        .map_err(|_| corrupt("invalid slot metadata sequence"))
}

fn valid_record_name(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|digits| {
        digits.len() == 16 && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn map_security(action: &'static str, path: &Path, error: StoreSecurityError) -> SlotMetadataError {
    match error {
        StoreSecurityError::Io(source) => SlotMetadataError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => SlotMetadataError::Corrupt(message),
    }
}

fn corrupt(message: &str) -> SlotMetadataError {
    SlotMetadataError::Corrupt(message.to_owned())
}
