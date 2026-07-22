#![doc = "Hash-protected package compatibility acceptance records."]

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::{AppIdentity, PackageName, PackageSupportLevel};
use crate::integrity::digest_json;
use crate::store_security;

const SCHEMA_VERSION: u32 = 1;

#[doc = "One verified package compatibility acceptance."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityPolicy {
    schema_version: u32,
    package: PackageName,
    uid: u32,
    signature_sha256: String,
    support_level: PackageSupportLevel,
    direct_boot_accepted: bool,
    sha256: String,
}

impl CompatibilityPolicy {
    fn new(
        package: PackageName,
        identity: &AppIdentity,
        support_level: PackageSupportLevel,
        direct_boot_accepted: bool,
    ) -> Result<Self, PolicyError> {
        if support_level == PackageSupportLevel::Blocked
            || direct_boot_accepted != (support_level == PackageSupportLevel::DirectBootConditional)
        {
            return Err(PolicyError::Corrupt(
                "invalid policy publication".to_owned(),
            ));
        }
        let mut value = Self {
            schema_version: SCHEMA_VERSION,
            package,
            uid: identity.uid(),
            signature_sha256: identity.signature_sha256().to_owned(),
            support_level,
            direct_boot_accepted,
            sha256: String::new(),
        };
        value.sha256 = value.digest()?;
        Ok(value)
    }

    fn verify(&self, package: &PackageName) -> Result<(), PolicyError> {
        if self.schema_version != SCHEMA_VERSION
            || &self.package != package
            || self.support_level == PackageSupportLevel::Blocked
            || self.direct_boot_accepted
                != (self.support_level == PackageSupportLevel::DirectBootConditional)
            || self.signature_sha256.len() != 64
            || !self
                .signature_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.digest()? != self.sha256
        {
            return Err(PolicyError::Corrupt(
                "invalid compatibility policy".to_owned(),
            ));
        }
        Ok(())
    }

    fn digest(&self) -> Result<String, PolicyError> {
        Ok(digest_json(&UnsignedPolicy {
            schema_version: self.schema_version,
            package: &self.package,
            uid: self.uid,
            signature_sha256: &self.signature_sha256,
            support_level: self.support_level,
            direct_boot_accepted: self.direct_boot_accepted,
        })?)
    }

    #[doc = "Returns whether the policy still accepts the installed identity and support class."]
    pub fn accepts(&self, identity: &AppIdentity, level: PackageSupportLevel) -> bool {
        self.uid == identity.uid()
            && self.signature_sha256 == identity.signature_sha256()
            && self.support_level == level
            && (level != PackageSupportLevel::DirectBootConditional || self.direct_boot_accepted)
    }
}

#[derive(Serialize)]
struct UnsignedPolicy<'a> {
    schema_version: u32,
    package: &'a PackageName,
    uid: u32,
    signature_sha256: &'a str,
    support_level: PackageSupportLevel,
    direct_boot_accepted: bool,
}

#[doc = "Owner-only immutable compatibility policy store."]
#[derive(Debug, Clone)]
pub struct CompatibilityPolicyStore {
    root: PathBuf,
    packages: PathBuf,
    owner_uid: u32,
}

impl CompatibilityPolicyStore {
    #[doc = "Creates or opens the fixed policy root."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid =
            store_security::initialize_root(&root, "compatibility policy").map_err(map_security)?;
        let packages = root.join("packages");
        store_security::ensure_child_directory(&packages, owner_uid, "compatibility policy")
            .map_err(map_security)?;
        Ok(Self {
            root,
            packages,
            owner_uid,
        })
    }

    #[doc = "Publishes the immutable enrollment-time support decision."]
    pub fn create(
        &self,
        package: &PackageName,
        identity: &AppIdentity,
        level: PackageSupportLevel,
        direct_boot_accepted: bool,
    ) -> Result<(), PolicyError> {
        self.validate_roots()?;
        let record =
            CompatibilityPolicy::new(package.clone(), identity, level, direct_boot_accepted)?;
        let directory = self.packages.join(package.as_str());
        store_security::ensure_child_directory(&directory, self.owner_uid, "compatibility policy")
            .map_err(map_security)?;
        let bytes = serde_json::to_vec(&record)?;
        store_security::write_new_record(
            &directory.join("policy.json"),
            &bytes,
            self.owner_uid,
            "compatibility policy",
        )
        .map_err(map_security)
    }

    #[doc = "Loads and verifies a policy, returning none for a legacy enrollment."]
    pub fn load(&self, package: &PackageName) -> Result<Option<CompatibilityPolicy>, PolicyError> {
        self.validate_roots()?;
        let directory = self.packages.join(package.as_str());
        let path = directory.join("policy.json");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PolicyError::Io(error)),
            Ok(_) => {}
        }
        store_security::validate_directory(&directory, self.owner_uid, "compatibility policy")
            .map_err(map_security)?;
        let bytes = store_security::read_record(&path, self.owner_uid, "compatibility policy")
            .map_err(map_security)?;
        let policy: CompatibilityPolicy = serde_json::from_slice(&bytes)?;
        policy.verify(package)?;
        Ok(Some(policy))
    }

    fn validate_roots(&self) -> Result<(), PolicyError> {
        store_security::validate_directory(&self.root, self.owner_uid, "compatibility policy")
            .and_then(|()| {
                store_security::validate_directory(
                    &self.packages,
                    self.owner_uid,
                    "compatibility policy",
                )
            })
            .map_err(map_security)
    }
}

#[doc = "Compatibility policy persistence failure."]
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[doc = "The policy structure or integrity digest is invalid."]
    #[error("corrupt compatibility policy: {0}")]
    Corrupt(String),
    #[doc = "A policy filesystem operation failed."]
    #[error("compatibility policy I/O: {0}")]
    Io(#[from] std::io::Error),
    #[doc = "A policy could not be encoded or decoded."]
    #[error("compatibility policy JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn map_security(error: store_security::StoreSecurityError) -> PolicyError {
    match error {
        store_security::StoreSecurityError::Io(source) => PolicyError::Io(source),
        store_security::StoreSecurityError::Corrupt(message) => PolicyError::Corrupt(message),
    }
}
