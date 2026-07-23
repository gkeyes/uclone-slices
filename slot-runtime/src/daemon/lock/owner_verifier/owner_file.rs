use std::fs::{self, OpenOptions};
use std::io::{self, Read as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use super::super::MAX_OWNER_BYTES;
use super::super::record::OwnerRecord;
use super::RuntimeOwnerMismatch;

#[allow(
    clippy::cast_possible_truncation,
    reason = "MAX_OWNER_BYTES is a fixed 512-byte protocol bound"
)]
const MAX_OWNER_BYTES_USIZE: usize = MAX_OWNER_BYTES as usize;

#[derive(Debug)]
pub(super) enum OwnerReadError {
    Missing,
    Mismatch(RuntimeOwnerMismatch),
    Io(io::Error),
}

pub(super) fn read_owner_record(
    path: &Path,
    owner_uid: u32,
) -> Result<OwnerRecord, OwnerReadError> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(OwnerReadError::Missing);
        }
        Err(error) => return Err(OwnerReadError::Io(error)),
    };
    if !trusted_record(&before, owner_uid) {
        return Err(OwnerReadError::Mismatch(record_shape(&before, owner_uid)));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(OwnerReadError::Io)?;
    let opened = file.metadata().map_err(OwnerReadError::Io)?;
    if !trusted_record(&opened, owner_uid) || !same_record(&before, &opened) {
        return Err(OwnerReadError::Mismatch(
            RuntimeOwnerMismatch::OwnerArtifact,
        ));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_OWNER_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(OwnerReadError::Io)?;
    if bytes.len() > MAX_OWNER_BYTES_USIZE {
        return Err(OwnerReadError::Mismatch(
            RuntimeOwnerMismatch::OwnerMalformed,
        ));
    }
    let after = fs::symlink_metadata(path).map_err(OwnerReadError::Io)?;
    if !trusted_record(&after, owner_uid) || !same_record(&opened, &after) {
        return Err(OwnerReadError::Mismatch(
            RuntimeOwnerMismatch::OwnerArtifact,
        ));
    }
    OwnerRecord::parse(&bytes)
        .map_err(|_| OwnerReadError::Mismatch(RuntimeOwnerMismatch::OwnerMalformed))
}

fn trusted_record(metadata: &fs::Metadata, owner_uid: u32) -> bool {
    metadata.file_type().is_file()
        && metadata.uid() == owner_uid
        && metadata.mode() & 0o7777 == 0o600
        && metadata.nlink() == 1
        && metadata.len() <= MAX_OWNER_BYTES
}

fn record_shape(metadata: &fs::Metadata, owner_uid: u32) -> RuntimeOwnerMismatch {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        RuntimeOwnerMismatch::OwnerArtifact
    } else if metadata.uid() != owner_uid
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
    {
        RuntimeOwnerMismatch::OwnerMetadata
    } else {
        RuntimeOwnerMismatch::OwnerMalformed
    }
}

fn same_record(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.uid() == right.uid()
        && left.mode() & 0o7777 == right.mode() & 0o7777
        && left.nlink() == right.nlink()
        && left.len() == right.len()
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
