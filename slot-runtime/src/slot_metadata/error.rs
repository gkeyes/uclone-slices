use std::io;
use std::path::{Path, PathBuf};

#[doc = "Slot metadata validation, integrity, transition, and I/O failures."]
#[derive(Debug, thiserror::Error)]
pub enum SlotMetadataError {
    #[doc = "A caller supplied an invalid display name, slot, version, or transition."]
    #[error("invalid slot metadata: {0}")]
    Invalid(String),
    #[doc = "A persisted stream failed structural or hash verification."]
    #[error("slot metadata is corrupt: {0}")]
    Corrupt(String),
    #[doc = "The immutable slot identifier already owns a stream."]
    #[error("slot metadata already exists")]
    AlreadyExists,
    #[doc = "The requested metadata stream is absent."]
    #[error("slot metadata was not found")]
    NotFound,
    #[doc = "A durable filesystem action failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "Stable operation label."]
        action: &'static str,
        #[doc = "Path associated with the failure."]
        path: PathBuf,
        #[doc = "Underlying filesystem error."]
        #[source]
        source: io::Error,
    },
    #[doc = "A metadata revision could not be encoded or decoded."]
    #[error("serialize slot metadata: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl SlotMetadataError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
