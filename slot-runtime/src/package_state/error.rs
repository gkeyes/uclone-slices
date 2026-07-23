use std::io;
use std::path::{Path, PathBuf};

use crate::domain::PackageKey;
use crate::lifecycle::LifecycleState;

#[doc = "Validation, lifecycle, and durable I/O failures for package state streams."]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageStateError {
    #[doc = "The package already has an initialized lifecycle stream."]
    #[error("package lifecycle state already exists: {0:?}")]
    AlreadyExists(PackageKey),
    #[doc = "The package has no initialized lifecycle stream."]
    #[error("package lifecycle state is not initialized: {0:?}")]
    NotInitialized(PackageKey),
    #[doc = "The caller's expected state differs from the latest durable state."]
    #[error("unexpected previous lifecycle state: expected {expected:?}, actual {actual:?}")]
    UnexpectedPrevious {
        #[doc = "State supplied by the caller."]
        expected: LifecycleState,
        #[doc = "Latest state in the durable stream."]
        actual: LifecycleState,
    },
    #[doc = "The caller's exact generation/digest compare-and-swap head is stale."]
    #[error(
        "unexpected package-state head: expected {expected_generation}/{expected_sha256}, actual {actual_generation}/{actual_sha256}"
    )]
    UnexpectedHead {
        #[doc = "Generation supplied by the caller."]
        expected_generation: u64,
        #[doc = "Digest supplied by the caller."]
        expected_sha256: String,
        #[doc = "Latest durable generation."]
        actual_generation: u64,
        #[doc = "Latest durable digest."]
        actual_sha256: String,
    },
    #[doc = "The schema-v1 head is unsafe to migrate into executable schema v2."]
    #[error("schema-v1 to schema-v2 migration refused from schema {schema_version} {state:?}")]
    MigrationRefused {
        #[doc = "Head schema version."]
        schema_version: u32,
        #[doc = "Legacy head lifecycle state."]
        state: LifecycleState,
    },
    #[doc = "The requested lifecycle transition is not legal."]
    #[error("illegal lifecycle transition from {previous:?} to {next:?}")]
    IllegalTransition {
        #[doc = "Latest durable state."]
        previous: LifecycleState,
        #[doc = "Requested next state."]
        next: LifecycleState,
    },
    #[doc = "The persisted stream has invalid JSON, structure, or hash-chain data."]
    #[error("package lifecycle state is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A durable filesystem operation failed."]
    #[error("{action} at {path}: {source}")]
    Io {
        #[doc = "The operation that failed."]
        action: &'static str,
        #[doc = "The filesystem path involved."]
        path: PathBuf,
        #[doc = "The underlying filesystem error."]
        #[source]
        source: io::Error,
    },
    #[doc = "A persisted revision could not be serialized or decoded."]
    #[error("serialize package lifecycle state: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl PackageStateError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
