use std::path::{Path, PathBuf};

use crate::domain::{PackageKey, UserId};

use super::{CatalogEntry, CatalogError, CatalogStore};

#[doc = "One exact immutable catalog manifest plus its securely-read file digest."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCatalogEntry {
    entry: CatalogEntry,
    sha256: String,
}

impl VerifiedCatalogEntry {
    #[doc = "Returns the decoded immutable catalog entry."]
    pub const fn entry(&self) -> &CatalogEntry {
        &self.entry
    }

    #[doc = "Returns the SHA-256 of the exact securely-read manifest file."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

impl CatalogStore {
    #[doc = "Securely reads only the derived base manifest without scanning Preview slots."]
    pub fn load_exact_base_secure(
        root: &Path,
        package_key: &PackageKey,
    ) -> Result<VerifiedCatalogEntry, CatalogError> {
        if package_key.user_id() != UserId::PRIMARY {
            return Err(CatalogError::Invalid(
                "secure base lookup supports only user zero".to_owned(),
            ));
        }
        let relative = PathBuf::from("packages")
            .join(package_key.package_name().as_str())
            .join("slots/base.json");
        let anchor = crate::rescue::anchor_file::read(root, &relative)
            .map_err(|error| CatalogError::Corrupt(error.to_string()))?;
        let entry = super::manifest::decode(&anchor.bytes)?;
        if entry.package_key() != package_key || !entry.slot_id().is_base() {
            return Err(CatalogError::Corrupt(
                "exact base manifest identity mismatch".to_owned(),
            ));
        }
        Ok(VerifiedCatalogEntry {
            entry,
            sha256: anchor.sha256,
        })
    }
}
