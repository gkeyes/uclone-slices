#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures exercise bounded IO helpers"
)]

use std::fs::{self, File, Metadata};
use std::io::{ErrorKind, Read as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use super::facts::FactError;

pub const MAX_CMDLINE_BYTES: usize = 4096;

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, FactError> {
    let file = File::open(path).map_err(|_| FactError::Unavailable)?;
    read_file_bounded(file, limit)
}

pub fn read_bounded_if_exists(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, FactError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(FactError::Unavailable),
    };
    read_file_bounded(file, limit).map(Some)
}

pub fn directory_inode(path: &Path) -> Result<u64, FactError> {
    optional_directory_metadata(path)?
        .map(|metadata| metadata.ino())
        .ok_or(FactError::Invalid)
}

pub fn optional_directory_inode(path: &Path) -> Result<Option<u64>, FactError> {
    optional_directory_metadata(path).map(|metadata| metadata.map(|value| value.ino()))
}

pub fn optional_directory_metadata(path: &Path) -> Result<Option<Metadata>, FactError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(FactError::Unavailable),
    };
    if !metadata.file_type().is_dir() || metadata.ino() == 0 {
        return Err(FactError::Invalid);
    }
    Ok(Some(metadata))
}

fn read_file_bounded(file: File, limit: usize) -> Result<Vec<u8>, FactError> {
    let limit = u64::try_from(limit).map_err(|_| FactError::Invalid)?;
    let observation_limit = limit.checked_add(1).ok_or(FactError::Invalid)?;
    let mut bytes = Vec::new();
    file.take(observation_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| FactError::Unavailable)?;
    if u64::try_from(bytes.len()).map_err(|_| FactError::Invalid)? > limit {
        Err(FactError::Invalid)
    } else {
        Ok(bytes)
    }
}
