use std::fs::{self, DirEntry, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use super::{PACKAGES, RESCUE, STEPS, StorePaths};
use crate::rescue::RescueError;

const DIRECTORY_MODE: u32 = 0o700;

pub(super) fn ensure_root(path: &Path) -> Result<u32, RescueError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|source| RescueError::io("create rescue root", path, source))?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(|source| RescueError::io("secure rescue root", path, source))?;
            if let Some(parent) = path.parent() {
                let _sync_result = sync_unchecked(parent);
            }
        }
        Err(source) => return Err(RescueError::io("inspect rescue root", path, source)),
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| RescueError::io("inspect rescue root", path, source))?;
    let owner_uid = metadata.uid();
    #[cfg(target_os = "android")]
    if owner_uid != 0 {
        return Err(RescueError::Corrupt(
            "fixed rescue root is not owned by root".to_owned(),
        ));
    }
    validate_directory(path, owner_uid)?;
    Ok(owner_uid)
}

pub(super) fn ensure_child_directory(path: &Path, owner_uid: u32) -> Result<(), RescueError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|source| RescueError::io("create rescue directory", path, source))?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(|source| RescueError::io("secure rescue directory", path, source))?;
            if let Some(parent) = path.parent() {
                sync_directory(parent, owner_uid)?;
            }
        }
        Err(source) => return Err(RescueError::io("inspect rescue directory", path, source)),
    }
    validate_directory(path, owner_uid)
}

pub(super) fn validate_store(paths: &StorePaths) -> Result<(), RescueError> {
    validate_directory(&paths.root, paths.owner_uid)?;
    exact_directory(&paths.root, PACKAGES, paths.owner_uid)?;
    validate_directory(&paths.packages, paths.owner_uid)?;
    let package_entries = bounded_entries(&paths.packages, 1)?;
    if package_entries.is_empty() {
        return Ok(());
    }
    let package = package_entries
        .first()
        .ok_or(RescueError::BoundExceeded("package artifacts"))?;
    if package.file_name() != crate::protocol::ALLOWED_PACKAGE {
        return Err(unexpected(&paths.packages));
    }
    validate_directory(&paths.package, paths.owner_uid)?;
    exact_directory(&paths.package, RESCUE, paths.owner_uid)?;
    validate_directory(&paths.rescue, paths.owner_uid)?;
    exact_directory(&paths.rescue, STEPS, paths.owner_uid)?;
    validate_directory(&paths.steps, paths.owner_uid)
}

pub(super) fn bounded_entries(path: &Path, maximum: usize) -> Result<Vec<DirEntry>, RescueError> {
    let entries = fs::read_dir(path)
        .map_err(|source| RescueError::io("read rescue directory", path, source))?;
    let mut result = Vec::new();
    for entry in entries {
        if result.len() == maximum {
            return Err(RescueError::BoundExceeded("directory artifacts"));
        }
        result.push(entry.map_err(|source| RescueError::io("read rescue artifact", path, source))?);
    }
    Ok(result)
}

pub(super) fn sync_directory(path: &Path, owner_uid: u32) -> Result<(), RescueError> {
    validate_directory(path, owner_uid)?;
    sync_unchecked(path).map_err(|source| RescueError::io("sync rescue directory", path, source))
}

fn exact_directory(parent: &Path, expected_name: &str, owner_uid: u32) -> Result<(), RescueError> {
    let entries = bounded_entries(parent, 1)?;
    let Some(entry) = entries.first() else {
        return Err(unexpected(parent));
    };
    let file_type = entry
        .file_type()
        .map_err(|source| RescueError::io("inspect rescue artifact", &entry.path(), source))?;
    if entry.file_name() != expected_name || !file_type.is_dir() {
        return Err(unexpected(parent));
    }
    validate_directory(&entry.path(), owner_uid)
}

fn validate_directory(path: &Path, owner_uid: u32) -> Result<(), RescueError> {
    let before = fs::symlink_metadata(path)
        .map_err(|source| RescueError::io("inspect rescue directory", path, source))?;
    if !before.file_type().is_dir()
        || before.uid() != owner_uid
        || before.mode() & 0o7777 != DIRECTORY_MODE
    {
        return Err(RescueError::Corrupt(format!(
            "untrusted rescue directory {}",
            path.display()
        )));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| RescueError::io("open rescue directory", path, source))?;
    let opened = file
        .metadata()
        .map_err(|source| RescueError::io("verify rescue directory", path, source))?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err(RescueError::Corrupt(
            "rescue directory changed during validation".to_owned(),
        ));
    }
    Ok(())
}

fn sync_unchecked(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)?
        .sync_all()
}

fn unexpected(path: &Path) -> RescueError {
    RescueError::Corrupt(format!("unexpected rescue artifact in {}", path.display()))
}

pub(super) const fn no_follow_flag() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x0000_0100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x0002_0000
    }
}
