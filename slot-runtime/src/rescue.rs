#![doc = "Independent append-only emergency native-base rescue journal."]

pub(crate) mod anchor_file;
mod anchors;
mod coordinator;
mod event;
mod execution;
mod metadata;
mod model;
mod offline;
mod offline_roots;
mod offline_service;
mod step;
mod storage;
mod store;
mod transaction;

use std::io;
use std::path::{Path, PathBuf};

pub use coordinator::{NoRescueFault, RescueFaultInjector};
pub use event::{RescueEvent, RescuePhase};
pub use execution::{RescueExecution, RescueStartup, StartupGateOutcome};
pub use metadata::{RescueMetadata, RescueMetadataSource};
pub use model::{RescueDisposition, RescueId, RescueSpec};
pub use offline::OfflineRescuePlatform;
pub use step::RescueStep;
pub use store::RescueJournalStore;
pub use transaction::{RescueStatus, RescueTransaction};

#[doc = "Fixed root read before any ordinary control-plane store is opened."]
pub const FIXED_RESCUE_JOURNAL_ROOT: &str = crate::target::RESCUE_JOURNAL_ROOT;

#[doc = "Rescue-journal validation, integrity, transition, and durable-I/O failures."]
#[derive(Debug, thiserror::Error)]
pub enum RescueError {
    #[doc = "The immutable rescue specification violates a safety invariant."]
    #[error("invalid rescue specification: {0}")]
    InvalidSpec(String),
    #[doc = "A caller attempted to use anything except the compiled user-zero package."]
    #[error("only the compiled allowlisted user-zero package is supported")]
    UnsupportedPackage,
    #[doc = "A durable rescue exists, but its immutable specification differs."]
    #[error("an existing rescue has a different immutable specification")]
    SpecMismatch,
    #[doc = "The requested event cannot follow the latest proven phase."]
    #[error("illegal rescue transition from {previous} to {next}")]
    IllegalTransition {
        #[doc = "Stable name of the latest proven phase."]
        previous: &'static str,
        #[doc = "Stable name of the rejected event."]
        next: &'static str,
    },
    #[doc = "A persisted artifact, hash chain, owner, mode, or shape is untrusted."]
    #[error("rescue journal is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A configured journal size or artifact-count bound was exceeded."]
    #[error("rescue journal bound exceeded: {0}")]
    BoundExceeded(&'static str),
    #[doc = "A filesystem durability operation failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "Stable operation label."]
        action: &'static str,
        #[doc = "Path whose operation failed."]
        path: PathBuf,
        #[doc = "Underlying I/O error."]
        #[source]
        source: io::Error,
    },
    #[doc = "A persisted domain value failed validation."]
    #[error(transparent)]
    Domain(#[from] crate::domain::DomainError),
    #[doc = "A rescue record could not be encoded."]
    #[error("serialize rescue journal record: {0}")]
    Serialize(#[from] serde_json::Error),
    #[doc = "A deterministic host test stopped execution without cleanup."]
    #[error("injected rescue crash after {0:?}")]
    InjectedCrash(RescuePhase),
}

impl RescueError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}

fn platform_error(error: impl Into<crate::runtime::PlatformError>) -> RescueError {
    RescueError::Corrupt(error.into().to_string())
}
