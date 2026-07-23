use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::domain::DomainError;

#[doc = "Borrowed immutable owner identity for a managed Android package."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppOwnerIdentityRef<'a> {
    uid: u32,
    signature_sha256: &'a str,
}

impl<'a> AppOwnerIdentityRef<'a> {
    #[doc = "Returns the Android application UID that owns the package data."]
    pub const fn uid(self) -> u32 {
        self.uid
    }

    #[doc = "Returns the lower-case signing certificate digest."]
    pub const fn signature_sha256(self) -> &'a str {
        self.signature_sha256
    }
}

#[doc = "Borrowed mutable installed-artifact facts for a package identity snapshot."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstalledArtifactRef<'a> {
    version_code: u64,
    code_path: &'a str,
}

impl<'a> InstalledArtifactRef<'a> {
    #[doc = "Returns the installed APK version code."]
    pub const fn version_code(self) -> u64 {
        self.version_code
    }

    #[doc = "Returns the canonical APK code path captured by `PackageManager`."]
    pub const fn code_path(self) -> &'a str {
        self.code_path
    }
}

#[doc = "Validated package observation containing immutable owner and installed-artifact facts."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AppIdentityRecord")]
pub struct AppIdentity {
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    code_path: String,
}

#[derive(Debug, Deserialize)]
struct AppIdentityRecord {
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    code_path: String,
}

impl TryFrom<AppIdentityRecord> for AppIdentity {
    type Error = DomainError;

    fn try_from(value: AppIdentityRecord) -> Result<Self, Self::Error> {
        Self::new(
            value.uid,
            &value.signature_sha256,
            value.version_code,
            &value.code_path,
        )
    }
}

impl AppIdentity {
    #[doc = "Constructs a validated ordinary application identity."]
    pub fn new(
        uid: u32,
        signature_sha256: &str,
        version_code: u64,
        code_path: &str,
    ) -> Result<Self, DomainError> {
        if uid < 10_000 {
            return Err(DomainError::InvalidAppUid(uid));
        }
        if signature_sha256.len() != 64
            || !signature_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(DomainError::InvalidSignatureDigest);
        }
        if version_code == 0 {
            return Err(DomainError::InvalidVersionCode(version_code));
        }
        validate_code_path(code_path)?;
        Ok(Self {
            uid,
            signature_sha256: signature_sha256.to_ascii_lowercase(),
            version_code,
            code_path: code_path.to_owned(),
        })
    }

    #[doc = "Returns the package UID."]
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    #[doc = "Returns the lower-case signing certificate digest."]
    pub fn signature_sha256(&self) -> &str {
        &self.signature_sha256
    }

    #[doc = "Returns the installed version code."]
    pub const fn version_code(&self) -> u64 {
        self.version_code
    }

    #[doc = "Returns the canonical APK code path captured by `PackageManager`."]
    pub fn code_path(&self) -> &str {
        &self.code_path
    }

    #[doc = "Returns the immutable owner portion without allocating."]
    pub fn owner_identity(&self) -> AppOwnerIdentityRef<'_> {
        AppOwnerIdentityRef {
            uid: self.uid,
            signature_sha256: &self.signature_sha256,
        }
    }

    #[doc = "Returns the mutable installed-artifact portion without allocating."]
    pub fn installed_artifact(&self) -> InstalledArtifactRef<'_> {
        InstalledArtifactRef {
            version_code: self.version_code,
            code_path: &self.code_path,
        }
    }
}

fn validate_code_path(raw: &str) -> Result<(), DomainError> {
    let path = Path::new(raw);
    if !path.is_absolute()
        || !raw.starts_with("/data/app/")
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
    {
        return Err(DomainError::InvalidCodePath(raw.to_owned()));
    }
    Ok(())
}
