use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file::{ensure_directory, sync_directory, write_new_synced};
use crate::domain::PackageKey;
use crate::integrity::digest_json;

use super::model::{EnrollmentAttempt, SCHEMA_VERSION};
use super::{CommittedAnchors, EnrollmentAttemptError};

mod shape;

pub(super) const GENERATIONS: &str = "generations";
pub(super) const MARKER: &str = "commit.marker";

pub(super) fn validate_package_directory(package_dir: &Path) -> Result<(), EnrollmentAttemptError> {
    shape::validate_package_directory(package_dir)
}

pub(super) fn valid_generation_name(name: &str) -> bool {
    shape::valid_generation_name(name)
}

pub(super) fn parse_generation(name: &str) -> Result<u64, EnrollmentAttemptError> {
    shape::parse_generation(name)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommitMarker {
    schema_version: u32,
    package_key: PackageKey,
    generation: u64,
    attempt_sha256: String,
    committed: CommittedAnchors,
    sha256: String,
}

impl CommitMarker {
    pub(super) fn new(
        package_key: &PackageKey,
        attempt: &EnrollmentAttempt,
        committed: &CommittedAnchors,
    ) -> Result<Self, EnrollmentAttemptError> {
        let mut marker = Self {
            schema_version: SCHEMA_VERSION,
            package_key: package_key.clone(),
            generation: attempt.generation(),
            attempt_sha256: attempt.sha256().to_owned(),
            committed: committed.clone(),
            sha256: String::new(),
        };
        marker.sha256 = marker.digest()?;
        Ok(marker)
    }

    pub(super) fn verify(
        &self,
        package_key: &PackageKey,
        attempt: &EnrollmentAttempt,
    ) -> Result<(), EnrollmentAttemptError> {
        if self.schema_version != SCHEMA_VERSION
            || self.package_key != *package_key
            || self.generation != attempt.generation()
            || self.attempt_sha256 != attempt.sha256()
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.digest()? != self.sha256
        {
            return Err(EnrollmentAttemptError::Corrupt(
                "commit marker does not match the latest generation".to_owned(),
            ));
        }
        if attempt.committed() != Some(&self.committed) {
            return Err(EnrollmentAttemptError::Corrupt(
                "commit marker anchors differ from the committed record".to_owned(),
            ));
        }
        Ok(())
    }

    fn digest(&self) -> Result<String, EnrollmentAttemptError> {
        #[derive(Serialize)]
        struct Unsigned<'a> {
            schema_version: u32,
            package_key: &'a PackageKey,
            generation: u64,
            attempt_sha256: &'a str,
            committed: &'a CommittedAnchors,
        }
        Ok(digest_json(&Unsigned {
            schema_version: self.schema_version,
            package_key: &self.package_key,
            generation: self.generation,
            attempt_sha256: &self.attempt_sha256,
            committed: &self.committed,
        })?)
    }
}

pub(super) fn ensure_root(root: &Path) -> Result<PathBuf, EnrollmentAttemptError> {
    reject_symlink_or_non_dir(root, "inspect enrollment attempt root")?;
    ensure_directory(root).map_err(|source| {
        EnrollmentAttemptError::io("create enrollment attempt root", root, source)
    })?;
    let attempts = root.join("attempts");
    reject_symlink_or_non_dir(&attempts, "inspect enrollment attempt directory")?;
    ensure_directory(&attempts).map_err(|source| {
        EnrollmentAttemptError::io("create enrollment attempt directory", &attempts, source)
    })?;
    Ok(attempts)
}

pub(super) fn load_generations(
    package_dir: &Path,
    package_key: &PackageKey,
) -> Result<Vec<EnrollmentAttempt>, EnrollmentAttemptError> {
    reject_symlink_or_non_dir(package_dir, "inspect enrollment attempt package")?;
    let generations = package_dir.join(GENERATIONS);
    reject_symlink_or_non_dir(&generations, "inspect enrollment attempt generations")?;
    let entries = fs::read_dir(&generations).map_err(|source| {
        EnrollmentAttemptError::io("read enrollment attempt generations", &generations, source)
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| {
            EnrollmentAttemptError::io("read enrollment attempt generation", &generations, source)
        })?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| {
            EnrollmentAttemptError::Corrupt("non-UTF-8 generation file".to_owned())
        })?;
        if !valid_generation_name(name) {
            return Err(EnrollmentAttemptError::Corrupt(format!(
                "unexpected enrollment attempt artifact {name}"
            )));
        }
        if !entry
            .file_type()
            .map_err(|source| {
                EnrollmentAttemptError::io("inspect enrollment generation", &entry.path(), source)
            })?
            .is_file()
        {
            return Err(EnrollmentAttemptError::Corrupt(
                "generation artifact is not a regular file".to_owned(),
            ));
        }
        files.push((parse_generation(name)?, entry.path()));
    }
    files.sort_by_key(|entry| entry.0);
    let mut loaded = Vec::with_capacity(files.len());
    for (file_generation, path) in files {
        let bytes = fs::read(&path).map_err(|source| {
            EnrollmentAttemptError::io("read enrollment attempt generation", &path, source)
        })?;
        let record = serde_json::from_slice::<EnrollmentAttempt>(&bytes).map_err(|source| {
            EnrollmentAttemptError::Corrupt(format!("invalid attempt JSON: {source}"))
        })?;
        if record.package_key() != package_key || record.generation() != file_generation {
            return Err(EnrollmentAttemptError::Corrupt(
                "generation package or filename mismatch".to_owned(),
            ));
        }
        record.verify(loaded.last())?;
        loaded.push(record);
    }
    if loaded.is_empty() {
        return Err(EnrollmentAttemptError::Corrupt(
            "enrollment attempt has no generations".to_owned(),
        ));
    }
    Ok(loaded)
}

pub(super) fn load_marker(
    package_dir: &Path,
) -> Result<Option<CommitMarker>, EnrollmentAttemptError> {
    let path = package_dir.join(MARKER);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(EnrollmentAttemptError::io(
                "inspect commit marker",
                &path,
                source,
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(EnrollmentAttemptError::Corrupt(
            "commit marker is not a regular file".to_owned(),
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|source| EnrollmentAttemptError::io("read commit marker", &path, source))?;
    let marker = serde_json::from_slice::<CommitMarker>(&bytes).map_err(|source| {
        EnrollmentAttemptError::Corrupt(format!("invalid commit marker: {source}"))
    })?;
    Ok(Some(marker))
}

pub(super) fn write_generation(
    package_dir: &Path,
    record: &EnrollmentAttempt,
) -> Result<(), EnrollmentAttemptError> {
    let generations = package_dir.join(GENERATIONS);
    let path = generations.join(format!("{:016}.json", record.generation()));
    let bytes = serde_json::to_vec(record)?;
    write_new_synced(&path, &bytes).map_err(|source| {
        EnrollmentAttemptError::io("publish enrollment generation", &path, source)
    })?;
    sync_directory(package_dir).map_err(|source| {
        EnrollmentAttemptError::io("sync enrollment attempt package", package_dir, source)
    })
}

pub(super) fn write_marker(
    package_dir: &Path,
    marker: &CommitMarker,
) -> Result<(), EnrollmentAttemptError> {
    let path = package_dir.join(MARKER);
    let bytes = serde_json::to_vec(marker)?;
    write_new_synced(&path, &bytes).map_err(|source| {
        EnrollmentAttemptError::io("publish enrollment commit marker", &path, source)
    })?;
    sync_directory(package_dir).map_err(|source| {
        EnrollmentAttemptError::io("sync enrollment commit marker", package_dir, source)
    })
}

pub(super) fn reject_symlink_or_non_dir(
    path: &Path,
    action: &'static str,
) -> Result<(), EnrollmentAttemptError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(EnrollmentAttemptError::Corrupt(format!(
            "{action}: path is not a real directory ({})",
            path.display()
        ))),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(EnrollmentAttemptError::io(action, path, source)),
    }
}
