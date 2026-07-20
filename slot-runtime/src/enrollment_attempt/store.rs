use std::path::{Path, PathBuf};

use crate::domain::{PackageKey, UserId};
use crate::layout::RuntimeLayout;
use crate::protocol::ALLOWED_PACKAGE;

use super::EnrollmentAttemptError;
use super::storage;

mod actions;

#[doc = "Filesystem-backed create-only enrollment-attempt store."]
#[derive(Debug, Clone)]
pub struct EnrollmentAttemptStore {
    pub(super) root: PathBuf,
    pub(super) attempts: PathBuf,
}

impl EnrollmentAttemptStore {
    #[doc = "Creates or opens a store below the supplied test/runtime root."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, EnrollmentAttemptError> {
        let root = root.as_ref().to_path_buf();
        let attempts = storage::ensure_root(&root)?;
        Ok(Self { root, attempts })
    }

    #[doc = "Opens the fixed compiled Preview runtime root."]
    pub fn fixed() -> Result<Self, EnrollmentAttemptError> {
        Self::new(RuntimeLayout::enrollment_attempt_root())
    }

    #[doc = "Returns the persistence root."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn package_path(&self, package: &crate::domain::PackageKey) -> PathBuf {
        self.attempts.join(package.package_name().as_str())
    }
}

pub(super) fn validate_key(package: &PackageKey) -> Result<(), EnrollmentAttemptError> {
    if package.user_id() != UserId::PRIMARY || package.package_name().as_str() != ALLOWED_PACKAGE {
        return Err(EnrollmentAttemptError::Invalid(
            "only compiled allowlisted user 0 is supported".to_owned(),
        ));
    }
    Ok(())
}
