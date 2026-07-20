use std::{fs, io};

use crate::atomic_file::sync_directory;
use crate::domain::PackageKey;

use super::super::super::EnrollmentAttemptError;
use super::super::super::storage;
use super::super::EnrollmentAttemptStore;

pub(super) fn remove_attempt(
    store: &EnrollmentAttemptStore,
    package: &PackageKey,
) -> Result<(), EnrollmentAttemptError> {
    let directory = store.package_path(package);
    let marker = directory.join(storage::MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(&marker).map_err(|source| {
                EnrollmentAttemptError::io("retire commit marker", &marker, source)
            })?;
        }
        Ok(_) => {
            return Err(EnrollmentAttemptError::Corrupt(
                "commit marker is not a regular file".to_owned(),
            ));
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(EnrollmentAttemptError::io(
                "inspect commit marker",
                &marker,
                source,
            ));
        }
    }
    let generations = directory.join(storage::GENERATIONS);
    for entry in fs::read_dir(&generations).map_err(|source| {
        EnrollmentAttemptError::io("read enrollment generations", &generations, source)
    })? {
        let entry = entry.map_err(|source| {
            EnrollmentAttemptError::io("read enrollment generation", &generations, source)
        })?;
        if !entry
            .file_type()
            .map_err(|source| {
                EnrollmentAttemptError::io("inspect enrollment generation", &entry.path(), source)
            })?
            .is_file()
        {
            return Err(EnrollmentAttemptError::Corrupt(
                "generation artifact is not a file".to_owned(),
            ));
        }
        fs::remove_file(entry.path()).map_err(|source| {
            EnrollmentAttemptError::io("retire enrollment generation", &generations, source)
        })?;
    }
    fs::remove_dir(&generations).map_err(|source| {
        EnrollmentAttemptError::io("retire enrollment generations", &generations, source)
    })?;
    fs::remove_dir(&directory).map_err(|source| {
        EnrollmentAttemptError::io("retire enrollment attempt", &directory, source)
    })?;
    sync_directory(&store.attempts).map_err(|source| {
        EnrollmentAttemptError::io("sync retired enrollment attempt", &store.attempts, source)
    })
}
