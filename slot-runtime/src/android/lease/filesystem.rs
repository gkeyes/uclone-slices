use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Read as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use crate::layout::RuntimeLayout;

use super::GateLeaseError;

const ADB_ROOT: &str = "/data/adb";
const DIRECTORY_MODE: u32 = 0o700;
const LEASE_MODE: u32 = 0o600;
const MAX_LEASE_BYTES: usize = 1024;
const MAX_LEASE_BYTES_U64: u64 = 1024;

#[cfg(any(target_os = "android", target_os = "linux"))]
const O_NOFOLLOW: i32 = 0o400_000;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const O_NOFOLLOW: i32 = 0x100;
#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    target_os = "ios"
)))]
const O_NOFOLLOW: i32 = 0;

pub(super) fn existing_gate_root() -> Result<Option<PathBuf>, GateLeaseError> {
    if !optional_directory(Path::new(ADB_ROOT))? {
        return Ok(None);
    }
    if !optional_directory(RuntimeLayout::root())? {
        return Ok(None);
    }
    let root = RuntimeLayout::gate_state_root();
    optional_directory(&root).map(|exists| exists.then_some(root))
}

pub(super) fn ensure_gate_root() -> Result<PathBuf, GateLeaseError> {
    require_directory(Path::new(ADB_ROOT))?;
    create_or_validate(RuntimeLayout::root())?;
    let root = RuntimeLayout::gate_state_root();
    create_or_validate(&root)?;
    Ok(root)
}

pub(super) fn path_present(path: &Path) -> Result<bool, GateLeaseError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(_) => Err(GateLeaseError::Unavailable),
    }
}

pub(super) fn read_secure_lease(path: &Path) -> Result<String, GateLeaseError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|_| GateLeaseError::UnsafeArtifact)?;
    let metadata = file.metadata().map_err(|_| GateLeaseError::Unavailable)?;
    if !safe_file_metadata(&metadata) || metadata.len() > MAX_LEASE_BYTES_U64 {
        return Err(GateLeaseError::UnsafeArtifact);
    }
    read_bounded(file)
}

fn read_bounded(file: File) -> Result<String, GateLeaseError> {
    let mut bytes = Vec::new();
    file.take(MAX_LEASE_BYTES_U64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| GateLeaseError::Unavailable)?;
    if bytes.len() > MAX_LEASE_BYTES {
        return Err(GateLeaseError::InvalidArtifact);
    }
    String::from_utf8(bytes).map_err(|_| GateLeaseError::InvalidArtifact)
}

fn create_or_validate(path: &Path) -> Result<(), GateLeaseError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_directory(&metadata),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| GateLeaseError::Unavailable)?;
            fs::set_permissions(path, fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(|_| GateLeaseError::Unavailable)?;
            require_directory(path)
        }
        Err(_) => Err(GateLeaseError::Unavailable),
    }
}

fn optional_directory(path: &Path) -> Result<bool, GateLeaseError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_directory(&metadata).map(|()| true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(_) => Err(GateLeaseError::Unavailable),
    }
}

fn require_directory(path: &Path) -> Result<(), GateLeaseError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| GateLeaseError::Unavailable)?;
    validate_directory(&metadata)
}

fn validate_directory(metadata: &Metadata) -> Result<(), GateLeaseError> {
    if metadata.file_type().is_dir()
        && metadata.uid() == 0
        && metadata.gid() == 0
        && metadata.mode() & 0o7777 == DIRECTORY_MODE
    {
        Ok(())
    } else {
        Err(GateLeaseError::UnsafeArtifact)
    }
}

fn safe_file_metadata(metadata: &Metadata) -> bool {
    metadata.file_type().is_file()
        && metadata.uid() == 0
        && metadata.gid() == 0
        && metadata.nlink() == 1
        && metadata.mode() & 0o7777 == LEASE_MODE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_permission_bits_are_not_accepted_as_a_lease_mode() {
        assert_eq!(0o600 & 0o7777, LEASE_MODE);
        assert_ne!(0o4600 & 0o7777, LEASE_MODE);
        assert_ne!(0o2600 & 0o7777, LEASE_MODE);
    }
}
