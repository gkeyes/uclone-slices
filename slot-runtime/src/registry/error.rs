use std::io;
use std::path::{Path, PathBuf};

#[doc = "Registry validation, integrity, and durable I/O failures."]
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[doc = "An unpublished revision violates Registry invariants."]
    #[error("invalid Registry revision: {0}")]
    InvalidRevision(String),
    #[doc = "A persisted Registry revision or hash chain is invalid."]
    #[error("Registry digest or structure is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A Registry filesystem durability operation failed."]
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
    #[doc = "A Registry revision could not be serialized."]
    #[error("serialize Registry revision: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl RegistryError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
