use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use super::{DIRECTORY_MODE, StoreSecurityError, no_follow_flag};

pub(crate) fn initialize_root(path: &Path, kind: &str) -> Result<u32, StoreSecurityError> {
    if !path.is_absolute() {
        return Err(StoreSecurityError::Corrupt(format!(
            "untrusted {kind} root path {}",
            path.display()
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        StoreSecurityError::Corrupt(format!("untrusted {kind} root path {}", path.display()))
    })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(StoreSecurityError::Io)?;
    let owner_uid = parent_metadata.uid();
    #[cfg(target_os = "android")]
    if owner_uid != 0 {
        return Err(StoreSecurityError::Corrupt(format!(
            "untrusted {kind} directory {}",
            parent.display()
        )));
    }
    validate_directory(parent, owner_uid, kind)?;
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(StoreSecurityError::Io)?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(StoreSecurityError::Io)?;
        }
        Err(source) => return Err(StoreSecurityError::Io(source)),
    }
    validate_directory(path, owner_uid, kind)?;
    Ok(owner_uid)
}

pub(crate) fn ensure_child_directory(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(StoreSecurityError::Io)?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(StoreSecurityError::Io)?;
        }
        Err(source) => return Err(StoreSecurityError::Io(source)),
    }
    validate_directory(path, owner_uid, kind)
}

pub(crate) fn validate_directory(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    let before = fs::symlink_metadata(path).map_err(StoreSecurityError::Io)?;
    if !trusted_directory(&before, owner_uid) {
        return Err(untrusted_directory(path, kind));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(StoreSecurityError::Io)?;
    let opened = file.metadata().map_err(StoreSecurityError::Io)?;
    if !trusted_directory(&opened, owner_uid)
        || before.dev() != opened.dev()
        || before.ino() != opened.ino()
    {
        return Err(untrusted_directory(path, kind));
    }
    Ok(())
}

pub(crate) fn sync_directory(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    validate_directory(path, owner_uid, kind)?;
    OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(StoreSecurityError::Io)?
        .sync_all()
        .map_err(StoreSecurityError::Io)
}

fn trusted_directory(metadata: &fs::Metadata, owner_uid: u32) -> bool {
    metadata.file_type().is_dir()
        && metadata.uid() == owner_uid
        && metadata.mode() & 0o7777 == DIRECTORY_MODE
}

fn untrusted_directory(path: &Path, kind: &str) -> StoreSecurityError {
    StoreSecurityError::Corrupt(format!("untrusted {kind} directory {}", path.display()))
}
