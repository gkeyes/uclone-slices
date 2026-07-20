use std::fs;
use std::io;
use std::path::Path;

use super::{PackageStateError, PackageStateRevision};
use crate::domain::{PackageName, UserId};
use crate::store_security::{self, StoreSecurityError};

pub(super) fn initialize_root(path: &Path) -> Result<u32, PackageStateError> {
    store_security::initialize_root(path, "package state")
        .map_err(|error| map_security("create package state root", path, error))
}

pub(super) fn ensure_directory(path: &Path, owner_uid: u32) -> Result<(), PackageStateError> {
    store_security::ensure_child_directory(path, owner_uid, "package state")
        .map_err(|error| map_security("create package state directory", path, error))
}

pub(super) fn validate_directory(path: &Path, owner_uid: u32) -> Result<(), PackageStateError> {
    store_security::validate_directory(path, owner_uid, "package state")
        .map_err(|error| map_security("validate package state directory", path, error))
}

pub(super) fn sync_directory(path: &Path, owner_uid: u32) -> Result<(), PackageStateError> {
    store_security::sync_directory(path, owner_uid, "package state")
        .map_err(|error| map_security("sync package state directory", path, error))
}

pub(super) fn load_all(
    revisions: &Path,
    package: &PackageName,
    owner_uid: u32,
) -> Result<Vec<PackageStateRevision>, PackageStateError> {
    if !validate_package_directory(revisions, owner_uid)? {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(revisions).map_err(|source| {
        PackageStateError::io("read package state revisions", revisions, source)
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| {
            PackageStateError::io("read package state revision entry", revisions, source)
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(PackageStateError::Corrupt(
                "non-UTF-8 revision file".to_owned(),
            ));
        };
        if !valid_revision_name(name) {
            return Err(PackageStateError::Corrupt(format!(
                "unexpected package state artifact {name}"
            )));
        }
        if !entry
            .file_type()
            .map_err(|source| {
                PackageStateError::io("inspect package state revision", &entry.path(), source)
            })?
            .is_file()
        {
            return Err(PackageStateError::Corrupt(format!(
                "revision artifact is not a file {name}"
            )));
        }
        files.push(entry.path());
    }
    files.sort();
    let mut loaded = Vec::with_capacity(files.len());
    for path in files {
        let file_generation = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(parse_generation)
            .ok_or_else(|| {
                PackageStateError::Corrupt("invalid revision filename generation".to_owned())
            })?;
        let bytes = store_security::read_record(&path, owner_uid, "package state revision")
            .map_err(|error| map_security("read package state revision", &path, error))?;
        let revision =
            serde_json::from_slice::<PackageStateRevision>(&bytes).map_err(|source| {
                PackageStateError::Corrupt(format!("invalid package state JSON: {source}"))
            })?;
        if revision.package_name() != package {
            return Err(PackageStateError::Corrupt(
                "package state directory mismatch".to_owned(),
            ));
        }
        if revision.generation() != file_generation {
            return Err(PackageStateError::Corrupt(
                "revision filename generation mismatch".to_owned(),
            ));
        }
        if revision.user_id() != UserId::PRIMARY {
            return Err(PackageStateError::Corrupt(
                "unsupported package state user".to_owned(),
            ));
        }
        revision.verify(loaded.last())?;
        loaded.push(revision);
    }
    Ok(loaded)
}

pub(super) fn write_revision(
    revisions: &Path,
    owner_uid: u32,
    revision: &PackageStateRevision,
) -> Result<(), PackageStateError> {
    let path = revisions.join(revision_file_name(revision.generation()));
    let bytes = serde_json::to_vec(revision)?;
    store_security::write_new_record(&path, &bytes, owner_uid, "package state revision")
        .map_err(|error| map_security("publish package state revision", &path, error))
}

fn validate_package_directory(revisions: &Path, owner_uid: u32) -> Result<bool, PackageStateError> {
    let Some(package_directory) = revisions.parent() else {
        return Err(PackageStateError::Corrupt(
            "package state revisions path has no package directory".to_owned(),
        ));
    };
    match fs::symlink_metadata(package_directory) {
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(PackageStateError::io(
                "inspect package lifecycle state directory",
                package_directory,
                source,
            ));
        }
        Ok(_) => {}
    }
    validate_directory(package_directory, owner_uid)?;
    let entries = fs::read_dir(package_directory).map_err(|source| {
        PackageStateError::io(
            "read package lifecycle state directory",
            package_directory,
            source,
        )
    })?;
    let mut found_revisions = false;
    for entry in entries {
        let entry = entry.map_err(|source| {
            PackageStateError::io(
                "read package lifecycle state entry",
                package_directory,
                source,
            )
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(PackageStateError::Corrupt(
                "non-UTF-8 package lifecycle state entry".to_owned(),
            ));
        };
        let file_type = entry.file_type().map_err(|source| {
            PackageStateError::io("inspect package state directory", &entry.path(), source)
        })?;
        if name != "revisions" || !file_type.is_dir() || found_revisions {
            return Err(PackageStateError::Corrupt(format!(
                "unexpected package state artifact {name}"
            )));
        }
        validate_directory(&entry.path(), owner_uid)?;
        found_revisions = true;
    }
    Ok(found_revisions)
}

pub(super) fn revision_file_name(generation: u64) -> String {
    format!("{generation:016}.json")
}

fn valid_revision_name(name: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".json") else {
        return false;
    };
    name.len() == 21 && prefix.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_generation(name: &str) -> Option<u64> {
    name.strip_suffix(".json")?.parse().ok()
}

fn map_security(action: &'static str, path: &Path, error: StoreSecurityError) -> PackageStateError {
    match error {
        StoreSecurityError::Io(source) => PackageStateError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => PackageStateError::Corrupt(message),
    }
}
