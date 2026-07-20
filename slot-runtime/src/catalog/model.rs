use crate::domain::{AppIdentity, DataInodes, PackageKey, SlotId};
use crate::layout::{RuntimeLayout, SlotPaths};

use super::CatalogError;

const MAX_CONTEXT_LENGTH: usize = 1_024;

#[doc = "Ownership, mode, `SELinux`, and fscrypt evidence for one data directory."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSecurityProof {
    uid: u32,
    gid: u32,
    mode: u32,
    selinux_context: String,
    fscrypt_policy_sha256: String,
}

impl PathSecurityProof {
    #[doc = "Validates one captured CE or DE security profile."]
    pub fn new(
        uid: u32,
        gid: u32,
        mode: u32,
        selinux_context: &str,
        fscrypt_policy_sha256: &str,
    ) -> Result<Self, CatalogError> {
        if mode > 0o7777 {
            return Err(CatalogError::Invalid(
                "directory mode exceeds 07777".to_owned(),
            ));
        }
        if selinux_context.is_empty()
            || selinux_context.len() > MAX_CONTEXT_LENGTH
            || selinux_context
                .bytes()
                .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
        {
            return Err(CatalogError::Invalid("invalid SELinux context".to_owned()));
        }
        if !valid_sha256(fscrypt_policy_sha256) {
            return Err(CatalogError::Invalid(
                "invalid fscrypt policy digest".to_owned(),
            ));
        }
        Ok(Self {
            uid,
            gid,
            mode,
            selinux_context: selinux_context.to_owned(),
            fscrypt_policy_sha256: fscrypt_policy_sha256.to_ascii_lowercase(),
        })
    }

    #[doc = "Returns the captured owner UID."]
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    #[doc = "Returns the captured owner GID."]
    pub const fn gid(&self) -> u32 {
        self.gid
    }

    #[doc = "Returns only the captured Unix permission and special bits."]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    #[doc = "Returns the captured `SELinux` context."]
    pub fn selinux_context(&self) -> &str {
        &self.selinux_context
    }

    #[doc = "Returns the lower-case SHA-256 digest of the fscrypt policy proof."]
    pub fn fscrypt_policy_sha256(&self) -> &str {
        &self.fscrypt_policy_sha256
    }
}

#[doc = "Paired CE and DE directory security evidence."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityProfileProof {
    ce: PathSecurityProof,
    de: PathSecurityProof,
}

impl SecurityProfileProof {
    #[doc = "Combines independently captured CE and DE evidence."]
    pub const fn new(ce: PathSecurityProof, de: PathSecurityProof) -> Self {
        Self { ce, de }
    }

    #[doc = "Returns the CE security evidence."]
    pub const fn ce(&self) -> &PathSecurityProof {
        &self.ce
    }

    #[doc = "Returns the DE security evidence."]
    pub const fn de(&self) -> &PathSecurityProof {
        &self.de
    }
}

#[doc = "One immutable base or non-base slot catalog entry."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    package_key: PackageKey,
    slot_id: SlotId,
    inodes: DataInodes,
    enrolled_identity: AppIdentity,
    created_version_code: u64,
    security_profile: SecurityProfileProof,
}

impl CatalogEntry {
    pub(super) fn new(
        package_key: PackageKey,
        slot_id: SlotId,
        inodes: DataInodes,
        enrolled_identity: AppIdentity,
        created_version_code: u64,
        security_profile: SecurityProfileProof,
    ) -> Result<Self, CatalogError> {
        if created_version_code == 0 {
            return Err(CatalogError::Invalid(
                "created version code must be non-zero".to_owned(),
            ));
        }
        if slot_id.is_base() && created_version_code != enrolled_identity.version_code() {
            return Err(CatalogError::Invalid(
                "base creation version must match enrollment".to_owned(),
            ));
        }
        if security_profile.ce().uid() != enrolled_identity.uid()
            || security_profile.de().uid() != enrolled_identity.uid()
        {
            return Err(CatalogError::Invalid(
                "CE/DE owner UID does not match enrolled app".to_owned(),
            ));
        }
        Ok(Self {
            package_key,
            slot_id,
            inodes,
            enrolled_identity,
            created_version_code,
            security_profile,
        })
    }

    #[doc = "Returns the package and supported Android user identity."]
    pub const fn package_key(&self) -> &PackageKey {
        &self.package_key
    }

    #[doc = "Returns the immutable slot identifier."]
    pub const fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    #[doc = "Returns the captured CE/DE inode pair."]
    pub const fn inodes(&self) -> DataInodes {
        self.inodes
    }

    #[doc = "Returns the immutable enrollment identity anchor."]
    pub const fn enrolled_identity(&self) -> &AppIdentity {
        &self.enrolled_identity
    }

    #[doc = "Returns the installed version code at slot creation."]
    pub const fn created_version_code(&self) -> u64 {
        self.created_version_code
    }

    #[doc = "Returns the captured CE/DE security evidence."]
    pub const fn security_profile(&self) -> &SecurityProfileProof {
        &self.security_profile
    }

    #[doc = "Derives trusted CE/DE source paths from the fixed runtime layout."]
    pub fn source_paths(&self) -> SlotPaths {
        RuntimeLayout::slot_paths(self.package_key.package_name(), &self.slot_id)
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
