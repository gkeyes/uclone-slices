use serde::{Deserialize, Serialize};

use crate::domain::{AppIdentity, DataInodes};

use super::super::EnrollmentAttemptError;

#[doc = "Committed identity and immutable base anchors protected by an attempt."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedAnchors {
    managed_sha256: String,
    identity: AppIdentity,
    base_inodes: DataInodes,
    enrollment_sha256: String,
    base_catalog_sha256: String,
    package_state_sha256: String,
}

impl CommittedAnchors {
    #[doc = "Returns the hash of the committed `ManagedPackage`."]
    pub fn managed_sha256(&self) -> &str {
        &self.managed_sha256
    }

    #[doc = "Returns the committed application identity."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }

    #[doc = "Returns the committed immutable base inode pair."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }

    #[doc = "Returns the published enrollment record digest."]
    pub fn enrollment_sha256(&self) -> &str {
        &self.enrollment_sha256
    }

    #[doc = "Returns the published base-catalog digest."]
    pub fn base_catalog_sha256(&self) -> &str {
        &self.base_catalog_sha256
    }

    #[doc = "Returns the published package-state digest."]
    pub fn package_state_sha256(&self) -> &str {
        &self.package_state_sha256
    }

    pub(super) const fn new(
        managed_sha256: String,
        identity: AppIdentity,
        base_inodes: DataInodes,
        enrollment_sha256: String,
        base_catalog_sha256: String,
        package_state_sha256: String,
    ) -> Self {
        Self {
            managed_sha256,
            identity,
            base_inodes,
            enrollment_sha256,
            base_catalog_sha256,
            package_state_sha256,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), EnrollmentAttemptError> {
        for (label, value) in [
            ("managed", self.managed_sha256.as_str()),
            ("enrollment", self.enrollment_sha256.as_str()),
            ("base catalog", self.base_catalog_sha256.as_str()),
            ("package state", self.package_state_sha256.as_str()),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(EnrollmentAttemptError::Corrupt(format!(
                    "{label} committed digest is invalid"
                )));
            }
        }
        Ok(())
    }
}
