use std::path::Path;

use crate::domain::{ManagedPackage, PackageName};
use crate::integrity::digest_bytes;
use crate::store_security;

use super::scan::validate_package_directory;
use super::store::EnrollmentStore;
use super::wire::EnrollmentRecord;
use super::{EnrollmentError, map_security};

#[doc = "One exact immutable enrollment plus the digest of its verified file bytes."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEnrollment {
    managed: ManagedPackage,
    sha256: String,
}

impl VerifiedEnrollment {
    #[doc = "Returns the decoded immutable base enrollment."]
    pub const fn managed(&self) -> &ManagedPackage {
        &self.managed
    }

    #[doc = "Returns the SHA-256 of the exact securely-read enrollment file."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

impl EnrollmentStore {
    #[doc = "Securely reads only the derived package enrollment without scanning siblings."]
    pub fn load_exact_secure(
        root: &Path,
        package_name: &PackageName,
    ) -> Result<VerifiedEnrollment, EnrollmentError> {
        let store = Self::open_existing(root)?;
        let package_dir = store.package_path(package_name);
        validate_package_directory(&package_dir, store.owner_uid())?;
        let path = package_dir.join("enrollment.json");
        let bytes = store_security::read_record(&path, store.owner_uid(), "enrollment record")
            .map_err(|error| map_security("read exact enrollment", &path, error))?;
        let record = serde_json::from_slice::<EnrollmentRecord>(&bytes)
            .map_err(|source| EnrollmentError::Corrupt(source.to_string()))?;
        record.verify()?;
        if record.managed.package_name() != package_name {
            return Err(EnrollmentError::Corrupt(
                "enrollment package does not match exact path".to_owned(),
            ));
        }
        Ok(VerifiedEnrollment {
            managed: record.managed,
            sha256: digest_bytes(&bytes),
        })
    }
}
