#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures exercise fixed filesystem readers"
)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use crate::android::MountCounts;
use crate::domain::{DataInodes, PackageName, SlotId};
use crate::layout::RuntimeLayout;

use super::facts::FactError;
use super::{io, parse};

const MIRROR_CE_ROOT: &str = "/data_mirror/data_ce";
const MIRROR_DE_ROOT: &str = "/data_mirror/data_de";
const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";
const MAX_MIRROR_ENTRIES: usize = 4096;

pub fn canonical_inodes(package: &PackageName) -> Result<DataInodes, FactError> {
    let paths = RuntimeLayout::slot_paths(package, &SlotId::base());
    required_pair(paths.ce(), paths.de())
}

pub fn mirror_inodes(package: &PackageName) -> Result<DataInodes, FactError> {
    mirror_inodes_at(
        Path::new(MIRROR_CE_ROOT),
        Path::new(MIRROR_DE_ROOT),
        package,
    )
}

pub fn mirror_inodes_at(
    ce_root: &Path,
    de_root: &Path,
    package: &PackageName,
) -> Result<DataInodes, FactError> {
    let ce = unique_mirror_match(ce_root, package)?;
    let de = unique_mirror_match(de_root, package)?;
    if ce.volume != de.volume {
        return Err(FactError::Invalid);
    }
    DataInodes::new(ce.inode, de.inode).map_err(|_| FactError::Invalid)
}

pub fn canonical_mount_counts(package: &PackageName) -> Result<MountCounts, FactError> {
    let bytes = io::read_bounded(Path::new(MOUNTINFO_PATH), parse::MAX_MOUNTINFO_BYTES)?;
    parse::parse_canonical_mount_counts(&bytes, package)
}

pub fn slot_inodes(package: &PackageName, slot: &SlotId) -> Result<Option<DataInodes>, FactError> {
    if slot.is_base() {
        return Err(FactError::Invalid);
    }
    let paths = RuntimeLayout::slot_paths(package, slot);
    slot_inodes_at(paths.ce(), paths.de())
}

pub fn slot_inodes_at(ce_path: &Path, de_path: &Path) -> Result<Option<DataInodes>, FactError> {
    let ce = io::optional_directory_inode(ce_path)?;
    let de = io::optional_directory_inode(de_path)?;
    match (ce, de) {
        (None, None) => Ok(None),
        (Some(ce), Some(de)) => DataInodes::new(ce, de)
            .map(Some)
            .map_err(|_| FactError::Invalid),
        (Some(_), None) | (None, Some(_)) => Err(FactError::Invalid),
    }
}

#[derive(Debug)]
struct MirrorMatch {
    volume: OsString,
    inode: u64,
}

fn unique_mirror_match(root: &Path, package: &PackageName) -> Result<MirrorMatch, FactError> {
    io::directory_inode(root)?;
    let entries = fs::read_dir(root).map_err(|_| FactError::Unavailable)?;
    let mut found = None;
    for (entry_index, entry) in entries.enumerate() {
        if entry_index >= MAX_MIRROR_ENTRIES {
            return Err(FactError::Invalid);
        }
        let entry = entry.map_err(|_| FactError::Unavailable)?;
        let volume = entry.file_name();
        validate_volume_name(&volume)?;
        let volume_path = entry.path();
        io::directory_inode(&volume_path)?;
        let user_path = volume_path.join("0");
        if io::optional_directory_inode(&user_path)?.is_none() {
            continue;
        }
        let package_path = user_path.join(package.as_str());
        let Some(inode) = io::optional_directory_inode(&package_path)? else {
            continue;
        };
        if found.is_some() {
            return Err(FactError::Invalid);
        }
        found = Some(MirrorMatch { volume, inode });
    }
    found.ok_or(FactError::Invalid)
}

fn validate_volume_name(name: &OsStr) -> Result<(), FactError> {
    let Some(name) = name.to_str() else {
        return Err(FactError::Invalid);
    };
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(FactError::Invalid);
    }
    Ok(())
}

fn required_pair(ce_path: &Path, de_path: &Path) -> Result<DataInodes, FactError> {
    let ce = io::directory_inode(ce_path)?;
    let de = io::directory_inode(de_path)?;
    DataInodes::new(ce, de).map_err(|_| FactError::Invalid)
}
