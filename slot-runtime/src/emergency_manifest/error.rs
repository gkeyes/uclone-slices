use std::io;
use std::path::{Path, PathBuf};

use super::ContainmentObligation;

/// Validation, transition, integrity, and durable-I/O failures for the emergency manifest.
#[derive(Debug, thiserror::Error)]
pub enum EmergencyManifestError {
    /// A newly constructed manifest violates a safety invariant.
    #[error("invalid emergency manifest: {0}")]
    InvalidManifest(String),
    /// A package obligation attempted an unapproved state transition.
    #[error("illegal emergency-manifest transition for {package}: {previous} -> {next}")]
    IllegalTransition {
        /// Package whose obligation changed.
        package: String,
        /// Previously proven obligation.
        previous: String,
        /// Requested next obligation.
        next: String,
    },
    /// A package was re-enrolled without the next identity fence.
    #[error("invalid enrollment epoch for {package}: expected {expected}, found {actual}")]
    EnrollmentEpoch {
        /// Package whose enrollment epoch changed unexpectedly.
        package: String,
        /// Epoch required by the previous committed record.
        expected: u64,
        /// Epoch supplied by the candidate record.
        actual: u64,
    },
    /// A manifest belongs to another boot epoch.
    #[error("stale boot id: expected {expected}, found {actual}")]
    StaleBoot {
        /// Boot epoch expected by the caller or previous commit.
        expected: String,
        /// Boot epoch found in the manifest.
        actual: String,
    },
    /// A manifest generation is not the one permitted after the committed record.
    #[error("stale generation: expected {expected}, found {actual}")]
    StaleGeneration {
        /// Generation required by the previous committed record.
        expected: u64,
        /// Generation supplied by the caller.
        actual: u64,
    },
    /// A committed artifact is not trusted and must not be interpreted.
    #[error("emergency manifest is corrupt: {0}")]
    Corrupt(String),
    /// A serialized manifest exceeded the bounded record size.
    #[error("emergency manifest bound exceeded: {0}")]
    BoundExceeded(&'static str),
    /// A filesystem operation failed.
    #[error("{action} at {path}: {source}")]
    Io {
        /// Stable operation label.
        action: &'static str,
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// JSON serialization failed before publication.
    #[error("serialize emergency manifest: {0}")]
    Serialize(#[from] serde_json::Error),
    /// A validated domain value could not be decoded from a persisted record.
    #[error(transparent)]
    Domain(#[from] crate::domain::DomainError),
}

impl EmergencyManifestError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }

    pub(super) fn illegal(
        package: &str,
        previous: ContainmentObligation,
        next: ContainmentObligation,
    ) -> Self {
        Self::IllegalTransition {
            package: package.to_owned(),
            previous: previous.as_str().to_owned(),
            next: next.as_str().to_owned(),
        }
    }
}
