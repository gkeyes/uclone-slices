#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-visible items are consumed by sibling enrollment and catalog modules"
)]

use std::fs::{self, OpenOptions};
use std::io::Read as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use super::RescueError;
use crate::integrity::digest_bytes;

const MAX_ANCHOR_BYTES: u64 = 1024 * 1024;

pub(crate) struct SecureAnchor {
    pub(crate) bytes: Vec<u8>,
    pub(crate) sha256: String,
}

pub(crate) fn read(root: &Path, relative: &Path) -> Result<SecureAnchor, RescueError> {
    let owner = validate_directory(root, None)?;
    #[cfg(target_os = "android")]
    if owner != 0 {
        return Err(corrupt("anchor root is not root-owned"));
    }
    validate_ancestors(root, relative, owner)?;
    let path = root.join(relative);
    let before = checked_file(&path, owner)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(&path)
        .map_err(|source| RescueError::io("open rescue anchor", &path, source))?;
    let opened = file
        .metadata()
        .map_err(|source| RescueError::io("inspect rescue anchor fd", &path, source))?;
    same_file(&before, &opened)?;
    let mut bytes = Vec::new();
    file.take(MAX_ANCHOR_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| RescueError::io("read rescue anchor", &path, source))?;
    if u64::try_from(bytes.len()).map_err(|_| RescueError::BoundExceeded("anchor bytes"))?
        > MAX_ANCHOR_BYTES
    {
        return Err(RescueError::BoundExceeded("anchor bytes"));
    }
    let after = checked_file(&path, owner)?;
    same_file(&opened, &after)?;
    Ok(SecureAnchor {
        sha256: digest_bytes(&bytes),
        bytes,
    })
}

fn validate_ancestors(root: &Path, relative: &Path, owner: u32) -> Result<(), RescueError> {
    let Some(parent) = relative.parent() else {
        return Err(corrupt("anchor path has no parent"));
    };
    let mut current = root.to_path_buf();
    for component in parent.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(corrupt("anchor path escapes fixed root"));
        };
        current.push(name);
        validate_directory(&current, Some(owner))?;
    }
    Ok(())
}

fn validate_directory(path: &Path, expected: Option<u32>) -> Result<u32, RescueError> {
    let before = fs::symlink_metadata(path)
        .map_err(|source| RescueError::io("inspect rescue anchor directory", path, source))?;
    let owner = before.uid();
    if !before.file_type().is_dir()
        || expected.is_some_and(|value| value != owner)
        || before.mode() & 0o022 != 0
    {
        return Err(corrupt("untrusted rescue anchor directory"));
    }
    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| RescueError::io("open rescue anchor directory", path, source))?
        .metadata()
        .map_err(|source| RescueError::io("verify rescue anchor directory", path, source))?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err(corrupt("rescue anchor directory changed"));
    }
    Ok(owner)
}

fn checked_file(path: &Path, owner: u32) -> Result<fs::Metadata, RescueError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| RescueError::io("inspect rescue anchor", path, source))?;
    if !metadata.file_type().is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() > MAX_ANCHOR_BYTES
    {
        return Err(corrupt("untrusted rescue anchor file"));
    }
    Ok(metadata)
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> Result<(), RescueError> {
    if left.dev() != right.dev()
        || left.ino() != right.ino()
        || left.mode() != right.mode()
        || left.uid() != right.uid()
        || left.nlink() != right.nlink()
        || left.len() != right.len()
    {
        return Err(corrupt("rescue anchor changed during read"));
    }
    Ok(())
}

fn corrupt(detail: &str) -> RescueError {
    RescueError::Corrupt(detail.to_owned())
}

const fn no_follow_flag() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x0000_0100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x0002_0000
    }
}
