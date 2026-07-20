use std::io;
use std::path::{Path, PathBuf};

use crate::domain::PackageKey;

#[doc = "Validation, integrity, and durable I/O failures for enrollment attempts."]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnrollmentAttemptError {
    #[doc = "An attempt already exists for the allowlisted package."]
    #[error("enrollment attempt already exists: {0:?}")]
    AlreadyExists(PackageKey),
    #[doc = "No attempt exists for the requested allowlisted package."]
    #[error("enrollment attempt not found: {0:?}")]
    NotFound(PackageKey),
    #[doc = "The operation is invalid for the current attempt phase."]
    #[error("invalid enrollment attempt operation: {0}")]
    Invalid(String),
    #[doc = "The commit proof does not prove all required durable publications."]
    #[error("enrollment commit proof is incomplete: {0}")]
    IncompleteCommitProof(String),
    #[doc = "The retirement proof does not prove exact restore and lease retirement."]
    #[error("enrollment retirement proof is incomplete: {0}")]
    IncompleteRetirementProof(String),
    #[doc = "A persisted attempt, generation, marker, or directory is corrupt."]
    #[error("enrollment attempt digest or structure is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A durable filesystem operation failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "The operation that failed."]
        action: &'static str,
        #[doc = "The filesystem path involved."]
        path: PathBuf,
        #[doc = "The underlying filesystem failure."]
        #[source]
        source: io::Error,
    },
    #[doc = "A persisted attempt could not be serialized or decoded."]
    #[error("serialize enrollment attempt: {0}")]
    Serialize(#[from] serde_json::Error),
    #[doc = "A domain value inside a proof was invalid."]
    #[error(transparent)]
    Domain(#[from] crate::domain::DomainError),
}

impl EnrollmentAttemptError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
