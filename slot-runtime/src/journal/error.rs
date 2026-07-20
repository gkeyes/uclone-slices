use std::io;
use std::path::{Path, PathBuf};

use crate::domain::{DomainError, TransactionId};

#[doc = "Journal validation, ordering, and durable I/O failures."]
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[doc = "The immutable transaction specification violates a safety invariant."]
    #[error("invalid transaction specification: {0}")]
    InvalidSpec(String),
    #[doc = "A final transaction directory already exists for this id."]
    #[error("transaction already exists: {0}")]
    AlreadyExists(TransactionId),
    #[doc = "An incomplete transaction staging directory already exists."]
    #[error("transaction staging already exists: {0}")]
    StagingExists(PathBuf),
    #[doc = "The next event is not legal after the latest durable event."]
    #[error("illegal journal transition from {previous} to {next}")]
    IllegalTransition {
        #[doc = "Stable name of the latest durable event."]
        previous: &'static str,
        #[doc = "Stable name of the rejected event."]
        next: &'static str,
    },
    #[doc = "The journal's persisted JSON, hash chain, or structure is invalid."]
    #[error("journal digest or structure is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A filesystem durability operation failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "The operation that failed."]
        action: &'static str,
        #[doc = "The filesystem path involved."]
        path: PathBuf,
        #[doc = "The underlying I/O error."]
        #[source]
        source: io::Error,
    },
    #[doc = "A persisted domain value failed validation."]
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[doc = "A journal record could not be serialized."]
    #[error("serialize journal record: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl JournalError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
