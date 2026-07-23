use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use super::storage;
use super::{EmergencyManifestError, no_follow_flag};

const FILE_MODE: u32 = 0o600;

pub(super) fn initialize(path: &Path, owner_uid: u32) -> Result<(), EmergencyManifestError> {
    let created = fs::symlink_metadata(path).is_err();
    let file = open(path, owner_uid)?;
    drop(file);
    if created {
        let parent = path.parent().ok_or_else(|| {
            EmergencyManifestError::Corrupt("manifest lock has no parent".to_owned())
        })?;
        storage::sync_directory(parent, owner_uid)?;
    }
    Ok(())
}

pub(super) fn acquire(path: &Path, owner_uid: u32) -> Result<File, EmergencyManifestError> {
    let file = open(path, owner_uid)?;
    file.lock()
        .map_err(|source| EmergencyManifestError::io("lock emergency manifest", path, source))?;
    Ok(file)
}

fn open(path: &Path, owner_uid: u32) -> Result<File, EmergencyManifestError> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .mode(FILE_MODE)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| EmergencyManifestError::io("open manifest lock", path, source))?;
    file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
        .map_err(|source| EmergencyManifestError::io("protect manifest lock", path, source))?;
    let metadata = file
        .metadata()
        .map_err(|source| EmergencyManifestError::io("inspect manifest lock", path, source))?;
    if !metadata.file_type().is_file()
        || metadata.uid() != owner_uid
        || metadata.mode() & 0o7777 != FILE_MODE
        || metadata.nlink() != 1
        || metadata.len() != 0
    {
        return Err(EmergencyManifestError::Corrupt(format!(
            "untrusted emergency manifest lock {}",
            path.display()
        )));
    }
    Ok(file)
}
