use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{DataInodes, PackageKey, SlotId};

use super::manifest::decode;
use super::{CatalogEntry, CatalogError};

pub(super) fn load_package(
    package_dir: &Path,
    package_key: &PackageKey,
) -> Result<Vec<CatalogEntry>, CatalogError> {
    let slots = validate_package_directory(package_dir)?;
    let mut files = read_slot_files(&slots)?;
    files.sort_by(|left, right| match (left.0.is_base(), right.0.is_base()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.0.cmp(&right.0),
    });
    let mut entries = Vec::with_capacity(files.len());
    for (file_slot, path) in files {
        let bytes = fs::read(&path)
            .map_err(|source| CatalogError::io("read slot manifest", &path, source))?;
        let entry = decode(&bytes)?;
        if entry.package_key() != package_key {
            return Err(CatalogError::Corrupt(
                "package directory mismatch".to_owned(),
            ));
        }
        if entry.slot_id() != &file_slot {
            return Err(CatalogError::Corrupt(format!(
                "slot filename mismatch at {}",
                path.display()
            )));
        }
        entries.push(entry);
    }
    validate_entries(package_key, &entries)?;
    Ok(entries)
}

fn validate_package_directory(package_dir: &Path) -> Result<PathBuf, CatalogError> {
    let mut entries = fs::read_dir(package_dir)
        .map_err(|source| CatalogError::io("read package catalog", package_dir, source))?;
    let Some(entry) = entries.next() else {
        return Err(CatalogError::Corrupt("missing slots directory".to_owned()));
    };
    let entry =
        entry.map_err(|source| CatalogError::io("read catalog artifact", package_dir, source))?;
    let file_type = entry
        .file_type()
        .map_err(|source| CatalogError::io("inspect catalog artifact", &entry.path(), source))?;
    if entry.file_name() != "slots" || !file_type.is_dir() || entries.next().is_some() {
        return Err(CatalogError::Corrupt(format!(
            "unexpected catalog artifact in {}",
            package_dir.display()
        )));
    }
    Ok(entry.path())
}

fn read_slot_files(slots: &Path) -> Result<Vec<(SlotId, PathBuf)>, CatalogError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(slots)
        .map_err(|source| CatalogError::io("read slot manifests", slots, source))?
    {
        let entry =
            entry.map_err(|source| CatalogError::io("read slot artifact", slots, source))?;
        let file_type = entry
            .file_type()
            .map_err(|source| CatalogError::io("inspect slot artifact", &entry.path(), source))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| CatalogError::Corrupt("non-UTF-8 slot artifact".to_owned()))?;
        let slot = name
            .strip_suffix(".json")
            .and_then(|stem| SlotId::parse(stem).ok())
            .ok_or_else(|| CatalogError::Corrupt(format!("unexpected slot artifact {name}")))?;
        if !file_type.is_file() {
            return Err(CatalogError::Corrupt(format!(
                "unexpected slot artifact {name}"
            )));
        }
        files.push((slot, entry.path()));
    }
    Ok(files)
}

fn validate_entries(key: &PackageKey, entries: &[CatalogEntry]) -> Result<(), CatalogError> {
    let Some(base) = entries.iter().find(|entry| entry.slot_id().is_base()) else {
        return Err(CatalogError::Corrupt(format!(
            "missing base manifest for {}",
            key.package_name()
        )));
    };
    for (index, entry) in entries.iter().enumerate() {
        if entry.enrolled_identity() != base.enrolled_identity() {
            return Err(CatalogError::Corrupt(
                "enrollment identity mismatch".to_owned(),
            ));
        }
        if !entry.slot_id().is_base() && shares_inodes(entry.inodes(), base.inodes()) {
            return Err(CatalogError::Corrupt("base inode reuse".to_owned()));
        }
        if entries
            .iter()
            .take(index)
            .any(|previous| shares_inodes(previous.inodes(), entry.inodes()))
        {
            return Err(CatalogError::Corrupt("duplicate slot inode".to_owned()));
        }
    }
    Ok(())
}

fn shares_inodes(left: DataInodes, right: DataInodes) -> bool {
    left.ce() == right.ce() || left.de() == right.de()
}
