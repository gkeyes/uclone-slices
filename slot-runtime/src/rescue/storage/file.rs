use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::shape::{no_follow_flag, sync_directory};
use super::{MAX_RECORD_BYTES, MAX_RECORD_BYTES_U64};
use crate::rescue::RescueError;

const FILE_MODE: u32 = 0o600;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn write_new(path: &Path, bytes: &[u8], owner_uid: u32) -> Result<(), RescueError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(RescueError::BoundExceeded("rescue record bytes"));
    }
    reject_existing(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| RescueError::Corrupt("rescue step has no parent".to_owned()))?;
    sync_directory(parent, owner_uid)?;
    let temporary = temporary_path(path)?;
    let result = publish(&temporary, path, parent, bytes, owner_uid);
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn read_bounded(path: &Path, owner_uid: u32) -> Result<Vec<u8>, RescueError> {
    let before = checked_metadata(path, owner_uid)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| RescueError::io("open rescue step", path, source))?;
    let opened = file
        .metadata()
        .map_err(|source| RescueError::io("inspect open rescue step", path, source))?;
    verify_same_file(&before, &opened)?;
    let capacity = usize::try_from(opened.len())
        .map_err(|_| RescueError::BoundExceeded("rescue record bytes"))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)
        .map_err(|source| RescueError::io("read rescue step", path, source))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(RescueError::BoundExceeded("rescue record bytes"));
    }
    let after = checked_metadata(path, owner_uid)?;
    verify_same_file(&opened, &after)?;
    if opened.len() != after.len() {
        return Err(RescueError::Corrupt(
            "rescue step changed during read".to_owned(),
        ));
    }
    Ok(bytes)
}

fn publish(
    temporary: &Path,
    final_path: &Path,
    parent: &Path,
    bytes: &[u8],
    owner_uid: u32,
) -> Result<(), RescueError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(FILE_MODE)
        .custom_flags(no_follow_flag())
        .open(temporary)
        .map_err(|source| RescueError::io("create rescue temporary", temporary, source))?;
    file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
        .map_err(|source| RescueError::io("secure rescue temporary", temporary, source))?;
    verify_open_file(&file, owner_uid)?;
    file.write_all(bytes)
        .map_err(|source| RescueError::io("write rescue temporary", temporary, source))?;
    file.sync_all()
        .map_err(|source| RescueError::io("sync rescue temporary", temporary, source))?;
    drop(file);
    fs::hard_link(temporary, final_path)
        .map_err(|source| RescueError::io("publish rescue step", final_path, source))?;
    sync_directory(parent, owner_uid)?;
    fs::remove_file(temporary)
        .map_err(|source| RescueError::io("retire rescue temporary", temporary, source))?;
    sync_directory(parent, owner_uid)?;
    let _metadata = checked_metadata(final_path, owner_uid)?;
    Ok(())
}

fn checked_metadata(path: &Path, owner_uid: u32) -> Result<fs::Metadata, RescueError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| RescueError::io("inspect rescue step", path, source))?;
    if !metadata.file_type().is_file()
        || metadata.uid() != owner_uid
        || metadata.mode() & 0o7777 != FILE_MODE
        || metadata.nlink() != 1
        || metadata.len() > MAX_RECORD_BYTES_U64
    {
        return Err(RescueError::Corrupt(format!(
            "untrusted rescue step {}",
            path.display()
        )));
    }
    Ok(metadata)
}

fn verify_open_file(file: &File, owner_uid: u32) -> Result<(), RescueError> {
    let metadata = file
        .metadata()
        .map_err(|source| RescueError::io("inspect rescue temporary", Path::new("<fd>"), source))?;
    if !metadata.file_type().is_file()
        || metadata.uid() != owner_uid
        || metadata.mode() & 0o7777 != FILE_MODE
        || metadata.nlink() != 1
    {
        return Err(RescueError::Corrupt(
            "untrusted rescue temporary".to_owned(),
        ));
    }
    Ok(())
}

fn verify_same_file(left: &fs::Metadata, right: &fs::Metadata) -> Result<(), RescueError> {
    if left.dev() != right.dev() || left.ino() != right.ino() {
        return Err(RescueError::Corrupt(
            "rescue step changed during validation".to_owned(),
        ));
    }
    Ok(())
}

fn reject_existing(path: &Path) -> Result<(), RescueError> {
    match fs::symlink_metadata(path) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(RescueError::Corrupt(
            "rescue step generation already exists".to_owned(),
        )),
        Err(source) => Err(RescueError::io("inspect rescue step", path, source)),
    }
}

fn temporary_path(path: &Path) -> Result<PathBuf, RescueError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| RescueError::Corrupt("invalid rescue step name".to_owned()))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id())))
}
