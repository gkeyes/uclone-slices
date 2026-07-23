use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use super::{EmergencyManifestError, no_follow_flag};

mod readonly;
mod temporary;

const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
pub(super) const MAX_RECORD_BYTES: usize = 64 * 1024;
#[allow(
    clippy::cast_possible_truncation,
    reason = "MAX_RECORD_BYTES is a fixed 64 KiB protocol bound"
)]
const MAX_RECORD_BYTES_U64: u64 = MAX_RECORD_BYTES as u64;

#[cfg(test)]
std::thread_local! {
    static DIRECTORY_SYNC_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod storage_tests;

pub(super) fn open_existing_root(path: &Path) -> Result<u32, EmergencyManifestError> {
    readonly::open_existing_root(path)
}

pub(super) fn read_record_with_fence(
    root: &Path,
    path: &Path,
    owner_uid: u32,
) -> Result<readonly::ReadRecord, EmergencyManifestError> {
    readonly::read_record_with_fence(root, path, owner_uid)
}

pub(super) fn initialize_root(path: &Path) -> Result<u32, EmergencyManifestError> {
    if !path.is_absolute() {
        return Err(EmergencyManifestError::Corrupt(format!(
            "untrusted emergency manifest root {}",
            path.display()
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        EmergencyManifestError::Corrupt("emergency manifest root has no parent".to_owned())
    })?;
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|source| EmergencyManifestError::io("inspect manifest parent", parent, source))?;
    let owner_uid = parent_metadata.uid();
    #[cfg(target_os = "android")]
    if owner_uid != 0 {
        return Err(EmergencyManifestError::Corrupt(
            "emergency manifest root is not root-owned".to_owned(),
        ));
    }
    validate_directory(parent, owner_uid)?;
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|source| {
                EmergencyManifestError::io("create manifest root", path, source)
            })?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE)).map_err(
                |source| EmergencyManifestError::io("protect manifest root", path, source),
            )?;
            sync_directory(parent, owner_uid)?;
        }
        Err(source) => {
            return Err(EmergencyManifestError::io(
                "inspect manifest root",
                path,
                source,
            ));
        }
    }
    validate_directory(path, owner_uid)?;
    Ok(owner_uid)
}

pub(super) fn validate_directory(
    path: &Path,
    owner_uid: u32,
) -> Result<(), EmergencyManifestError> {
    let before = fs::symlink_metadata(path)
        .map_err(|source| EmergencyManifestError::io("inspect manifest directory", path, source))?;
    if !trusted_directory(&before, owner_uid) {
        return Err(untrusted_directory(path));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| EmergencyManifestError::io("open manifest directory", path, source))?;
    let opened = file.metadata().map_err(|source| {
        EmergencyManifestError::io("inspect opened manifest directory", path, source)
    })?;
    if !trusted_directory(&opened, owner_uid)
        || before.dev() != opened.dev()
        || before.ino() != opened.ino()
    {
        return Err(untrusted_directory(path));
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path, owner_uid: u32) -> Result<(), EmergencyManifestError> {
    #[cfg(test)]
    DIRECTORY_SYNC_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
    validate_directory(path, owner_uid)?;
    OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| {
            EmergencyManifestError::io("open manifest directory for sync", path, source)
        })?
        .sync_all()
        .map_err(|source| EmergencyManifestError::io("sync manifest directory", path, source))
}

pub(super) fn read_record(path: &Path, owner_uid: u32) -> Result<Vec<u8>, EmergencyManifestError> {
    let before = fs::symlink_metadata(path)
        .map_err(|source| EmergencyManifestError::io("inspect manifest record", path, source))?;
    validate_record_metadata(&before, path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| EmergencyManifestError::io("open manifest record", path, source))?;
    let opened = file.metadata().map_err(|source| {
        EmergencyManifestError::io("inspect opened manifest record", path, source)
    })?;
    validate_record_metadata(&opened, path)?;
    if !same_file(&before, &opened) || opened.uid() != owner_uid {
        return Err(untrusted_record(path));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(opened.len()).unwrap_or(MAX_RECORD_BYTES));
    std::io::Read::by_ref(&mut file)
        .take(MAX_RECORD_BYTES_U64.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| EmergencyManifestError::io("read manifest record", path, source))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(EmergencyManifestError::BoundExceeded("serialized record"));
    }
    let after = fs::symlink_metadata(path)
        .map_err(|source| EmergencyManifestError::io("recheck manifest record", path, source))?;
    validate_record_metadata(&after, path)?;
    if !same_file(&opened, &after) || opened.len() != after.len() {
        return Err(EmergencyManifestError::Corrupt(
            "manifest changed during read".to_owned(),
        ));
    }
    Ok(bytes)
}

pub(super) fn write_atomic(
    path: &Path,
    parent: &Path,
    bytes: &[u8],
    owner_uid: u32,
) -> Result<(), EmergencyManifestError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(EmergencyManifestError::BoundExceeded("serialized record"));
    }
    validate_directory(parent, owner_uid)?;
    let temporary = temporary::path(path)?;
    let result = publish(&temporary, path, parent, bytes, owner_uid);
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

fn publish(
    temporary: &Path,
    final_path: &Path,
    parent: &Path,
    bytes: &[u8],
    owner_uid: u32,
) -> Result<(), EmergencyManifestError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(FILE_MODE)
        .custom_flags(no_follow_flag())
        .open(temporary)
        .map_err(|source| {
            EmergencyManifestError::io("create manifest temporary", temporary, source)
        })?;
    file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
        .map_err(|source| {
            EmergencyManifestError::io("protect manifest temporary", temporary, source)
        })?;
    validate_open_record(&file, temporary, owner_uid)?;
    file.write_all(bytes).map_err(|source| {
        EmergencyManifestError::io("write manifest temporary", temporary, source)
    })?;
    file.sync_all().map_err(|source| {
        EmergencyManifestError::io("sync manifest temporary", temporary, source)
    })?;
    drop(file);
    fs::rename(temporary, final_path).map_err(|source| {
        EmergencyManifestError::io("publish emergency manifest", final_path, source)
    })?;
    sync_directory(parent, owner_uid)
}

fn validate_open_record(
    file: &File,
    path: &Path,
    owner_uid: u32,
) -> Result<(), EmergencyManifestError> {
    let metadata = file
        .metadata()
        .map_err(|source| EmergencyManifestError::io("inspect manifest temporary", path, source))?;
    validate_record_metadata(&metadata, path)?;
    if metadata.uid() != owner_uid {
        return Err(untrusted_record(path));
    }
    Ok(())
}

fn validate_record_metadata(
    metadata: &fs::Metadata,
    path: &Path,
) -> Result<(), EmergencyManifestError> {
    if metadata.file_type().is_file()
        && metadata.mode() & 0o7777 == FILE_MODE
        && metadata.nlink() == 1
        && metadata.len() <= MAX_RECORD_BYTES_U64
    {
        Ok(())
    } else {
        Err(untrusted_record(path))
    }
}

fn trusted_directory(metadata: &fs::Metadata, owner_uid: u32) -> bool {
    metadata.file_type().is_dir()
        && metadata.uid() == owner_uid
        && metadata.mode() & 0o7777 == DIRECTORY_MODE
}

fn untrusted_directory(path: &Path) -> EmergencyManifestError {
    EmergencyManifestError::Corrupt(format!(
        "untrusted emergency manifest directory {}",
        path.display()
    ))
}

fn untrusted_record(path: &Path) -> EmergencyManifestError {
    EmergencyManifestError::Corrupt(format!(
        "untrusted emergency manifest record {}",
        path.display()
    ))
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}
