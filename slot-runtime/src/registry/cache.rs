use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use super::store::{
    map_security, revision_file_name, valid_revision_name, valid_temporary_revision_name,
    validate_directory,
};
use super::{PackageRevision, RegistryError};
use crate::store_security;

#[derive(Debug, Clone)]
pub(super) struct CachedChain {
    head: Option<PackageRevision>,
    records: BTreeMap<u64, FileStamp>,
}

impl CachedChain {
    pub(super) const fn head(&self) -> Option<&PackageRevision> {
        self.head.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    mode: u32,
    owner: u32,
    group: u32,
    links: u64,
}

pub(super) fn snapshot(
    revisions: &Path,
    owner_uid: u32,
    head: Option<&PackageRevision>,
) -> Result<CachedChain, RegistryError> {
    let records = scan(revisions, owner_uid)?;
    validate_shape(&records, head)?;
    validate_head(revisions, owner_uid, head)?;
    Ok(CachedChain {
        head: head.cloned(),
        records,
    })
}

pub(super) fn validate(
    revisions: &Path,
    owner_uid: u32,
    cached: &CachedChain,
) -> Result<(), RegistryError> {
    let records = scan(revisions, owner_uid)?;
    if records != cached.records {
        return Err(RegistryError::Corrupt(
            "Registry cached chain changed".to_owned(),
        ));
    }
    validate_shape(&records, cached.head.as_ref())?;
    validate_head(revisions, owner_uid, cached.head.as_ref())
}

pub(super) fn advance(
    cached: &mut CachedChain,
    path: &Path,
    owner_uid: u32,
    revision: PackageRevision,
) -> Result<(), RegistryError> {
    let expected = u64::try_from(cached.records.len())
        .ok()
        .and_then(|generation| generation.checked_add(1))
        .ok_or_else(|| RegistryError::Corrupt("Registry generation overflow".to_owned()))?;
    if revision.generation != expected {
        return Err(RegistryError::Corrupt(
            "Registry cache generation mismatch".to_owned(),
        ));
    }
    cached
        .records
        .insert(expected, record_stamp(path, owner_uid)?);
    cached.head = Some(revision);
    Ok(())
}

fn scan(revisions: &Path, owner_uid: u32) -> Result<BTreeMap<u64, FileStamp>, RegistryError> {
    validate_directory(revisions, owner_uid)?;
    let entries = fs::read_dir(revisions)
        .map_err(|source| RegistryError::io("read package revisions", revisions, source))?;
    let mut records = BTreeMap::new();
    for entry in entries {
        let entry =
            entry.map_err(|source| RegistryError::io("read registry entry", revisions, source))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| RegistryError::Corrupt("non-UTF-8 revision file".to_owned()))?;
        if name.starts_with('.') && valid_temporary_revision_name(name) {
            let _stamp = record_stamp(&entry.path(), owner_uid)?;
            continue;
        }
        let generation = revision_generation(name)?;
        if generation == 0
            || records
                .insert(generation, record_stamp(&entry.path(), owner_uid)?)
                .is_some()
        {
            return Err(RegistryError::Corrupt(
                "invalid Registry generation set".to_owned(),
            ));
        }
    }
    Ok(records)
}

fn validate_shape(
    records: &BTreeMap<u64, FileStamp>,
    head: Option<&PackageRevision>,
) -> Result<(), RegistryError> {
    let expected = head.map_or(0, |revision| revision.generation);
    let count = u64::try_from(records.len())
        .map_err(|_| RegistryError::Corrupt("Registry generation overflow".to_owned()))?;
    if count == expected && records.keys().next_back().copied().unwrap_or(0) == expected {
        Ok(())
    } else {
        Err(RegistryError::Corrupt(
            "Registry cached head drift".to_owned(),
        ))
    }
}

fn validate_head(
    revisions: &Path,
    owner_uid: u32,
    cached: Option<&PackageRevision>,
) -> Result<(), RegistryError> {
    let Some(cached) = cached else {
        return Ok(());
    };
    let path = revisions.join(revision_file_name(cached.generation));
    let bytes = store_security::read_record(&path, owner_uid, "Registry revision")
        .map_err(|error| map_security("read cached Registry head", &path, error))?;
    let stored = serde_json::from_slice::<PackageRevision>(&bytes)
        .map_err(|source| RegistryError::Corrupt(format!("invalid registry JSON: {source}")))?;
    if stored == *cached {
        Ok(())
    } else {
        Err(RegistryError::Corrupt(
            "Registry cached head changed".to_owned(),
        ))
    }
}

fn revision_generation(name: &str) -> Result<u64, RegistryError> {
    if !valid_revision_name(name) {
        return Err(RegistryError::Corrupt(format!(
            "unexpected registry artifact {name}"
        )));
    }
    name.strip_suffix(".json")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| RegistryError::Corrupt("invalid Registry generation".to_owned()))
}

fn record_stamp(path: &Path, owner_uid: u32) -> Result<FileStamp, RegistryError> {
    store_security::validate_record(path, owner_uid, "Registry revision")
        .map_err(|error| map_security("validate Registry revision", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| RegistryError::io("inspect Registry revision", path, source))?;
    Ok(FileStamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
        mode: metadata.mode(),
        owner: metadata.uid(),
        group: metadata.gid(),
        links: metadata.nlink(),
    })
}
