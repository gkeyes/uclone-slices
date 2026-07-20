use std::io;
use std::path::{Path, PathBuf};

use crate::domain::{PackageName, SlotId};

#[doc = "Slot catalog validation, integrity, and durable I/O failures."]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    #[doc = "A proposed catalog entry has invalid metadata."]
    #[error("invalid slot catalog entry: {0}")]
    Invalid(String),
    #[doc = "The requested immutable slot already exists."]
    #[error("slot already cataloged for {package}: {slot}")]
    DuplicateSlot {
        #[doc = "The package containing the duplicate slot."]
        package: PackageName,
        #[doc = "The duplicate slot identifier."]
        slot: SlotId,
    },
    #[doc = "A package has no immutable base catalog entry."]
    #[error("package has no base catalog: {0}")]
    MissingBase(PackageName),
    #[doc = "A slot proposal does not match the enrolled application identity."]
    #[error("slot enrollment identity mismatch for {0}")]
    IdentityMismatch(PackageName),
    #[doc = "A non-base slot reuses an Android-owned base inode."]
    #[error("slot {slot} reuses a base inode for {package}")]
    BaseInodeReuse {
        #[doc = "The affected package."]
        package: PackageName,
        #[doc = "The invalid non-base slot."]
        slot: SlotId,
    },
    #[doc = "Persisted catalog metadata or its filesystem shape is corrupt."]
    #[error("slot catalog digest or structure is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A durable filesystem operation failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "The failed operation."]
        action: &'static str,
        #[doc = "The filesystem path involved."]
        path: PathBuf,
        #[doc = "The underlying I/O error."]
        #[source]
        source: io::Error,
    },
    #[doc = "A manifest could not be serialized."]
    #[error("serialize slot catalog manifest: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl CatalogError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
