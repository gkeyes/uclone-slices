#![doc = "Immutable package enrollment records for the Slots Preview runtime."]

use std::io;
use std::path::{Path, PathBuf};

use crate::domain::{ManagedPackage, PackageName, UserId};
use crate::lifecycle::LifecycleState;
use crate::store_security::StoreSecurityError;

mod scan;
mod secure;
mod store;
mod wire;

pub use scan::EnrollmentNameScan;
pub use secure::VerifiedEnrollment;
pub use store::EnrollmentStore;

const SCHEMA_VERSION: u32 = 1;

#[doc = "Enrollment persistence and integrity failures."]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnrollmentError {
    #[doc = "The package is already enrolled."]
    #[error("package is already enrolled: {0}")]
    AlreadyExists(PackageName),
    #[doc = "The package is not in a valid base enrollment state."]
    #[error("invalid base enrollment: {0}")]
    Invalid(String),
    #[doc = "The persisted enrollment is corrupt or unsupported."]
    #[error("enrollment digest or structure is corrupt: {0}")]
    Corrupt(String),
    #[doc = "A durable filesystem operation failed."]
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
    #[doc = "The record could not be serialized."]
    #[error("serialize enrollment: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl EnrollmentError {
    pub(super) fn io(action: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}

fn validate_base(managed: &ManagedPackage) -> Result<(), String> {
    if managed.user_id() != UserId::PRIMARY {
        return Err("enrollment supports Android user 0 only".to_owned());
    }
    if !managed.active_slot().is_base() || managed.active_inodes() != managed.base_inodes() {
        return Err("package must be enrolled on the immutable base view".to_owned());
    }
    if managed.lifecycle_state() != LifecycleState::Normal {
        return Err("package lifecycle must be normal at enrollment".to_owned());
    }
    Ok(())
}

pub(super) fn map_security(
    action: &'static str,
    path: &Path,
    error: StoreSecurityError,
) -> EnrollmentError {
    match error {
        StoreSecurityError::Io(source) => EnrollmentError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => EnrollmentError::Corrupt(message),
    }
}
