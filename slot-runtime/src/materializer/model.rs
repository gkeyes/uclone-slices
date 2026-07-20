use crate::catalog::SecurityProfileProof;
use crate::domain::DataInodes;

use super::MaterializationProofError;

mod proofs;
mod result;

pub use proofs::{DomainCopyProof, MaterializationPaths, SlotMaterializationProof};
pub use result::MaterializationResult;

#[doc = "One Android application data encryption domain."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDomain {
    #[doc = "Credential-encrypted application data."]
    Ce,
    #[doc = "Device-encrypted application data."]
    De,
}

#[doc = "Observed fixed-path artifact shape before or after materialization."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactState {
    #[doc = "Neither staging nor ready artifacts exist."]
    Absent,
    #[doc = "Only an incomplete staging pair exists."]
    StagingOnly,
    #[doc = "Only a published ready pair exists."]
    ReadyOnly,
    #[doc = "Staging and ready artifacts coexist and are ambiguous."]
    Both,
}

#[doc = "Validated SHA-256 content-tree digest."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDigest(String);

impl ContentDigest {
    #[doc = "Validates one lower- or upper-case SHA-256 digest."]
    pub fn parse(raw: &str) -> Result<Self, MaterializationProofError> {
        if raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(raw.to_ascii_lowercase()))
        } else {
            Err(MaterializationProofError::InvalidDigest)
        }
    }

    #[doc = "Returns the normalized lower-case digest."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[doc = "Paired CE and DE content-tree digests."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentProof {
    ce: ContentDigest,
    de: ContentDigest,
}

impl ContentProof {
    #[doc = "Validates paired CE and DE digests."]
    pub fn new(ce: &str, de: &str) -> Result<Self, MaterializationProofError> {
        Ok(Self {
            ce: ContentDigest::parse(ce)?,
            de: ContentDigest::parse(de)?,
        })
    }

    #[doc = "Returns the CE content digest."]
    pub const fn ce(&self) -> &ContentDigest {
        &self.ce
    }

    #[doc = "Returns the DE content digest."]
    pub const fn de(&self) -> &ContentDigest {
        &self.de
    }
}

#[doc = "Filesystem-device and content anchor for one directory tree."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAnchor {
    device_id: u64,
    content: ContentDigest,
}

impl DirectoryAnchor {
    #[doc = "Constructs a non-zero filesystem-device anchor."]
    pub fn new(device_id: u64, content: &str) -> Result<Self, MaterializationProofError> {
        if device_id == 0 {
            return Err(MaterializationProofError::InvalidDevice);
        }
        Ok(Self {
            device_id,
            content: ContentDigest::parse(content)?,
        })
    }

    #[doc = "Returns the filesystem device identifier."]
    pub const fn device_id(&self) -> u64 {
        self.device_id
    }

    #[doc = "Returns the directory content digest."]
    pub const fn content(&self) -> &ContentDigest {
        &self.content
    }
}

#[doc = "Immutable base proof sampled before and after materialization."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseAnchor {
    inodes: DataInodes,
    ce: DirectoryAnchor,
    de: DirectoryAnchor,
    security: SecurityProfileProof,
}

impl BaseAnchor {
    #[doc = "Combines inode, device, content, MCS, and fscrypt base evidence."]
    pub fn new(
        inodes: DataInodes,
        ce: DirectoryAnchor,
        de: DirectoryAnchor,
        security: SecurityProfileProof,
    ) -> Result<Self, MaterializationProofError> {
        if ce.device_id() == de.device_id() && inodes.ce() == inodes.de() {
            return Err(MaterializationProofError::DuplicateDirectoryIdentity);
        }
        Ok(Self {
            inodes,
            ce,
            de,
            security,
        })
    }

    #[doc = "Returns the base CE/DE inode pair."]
    pub const fn inodes(&self) -> DataInodes {
        self.inodes
    }

    #[doc = "Returns one domain directory anchor."]
    pub const fn domain(&self, domain: DataDomain) -> &DirectoryAnchor {
        match domain {
            DataDomain::Ce => &self.ce,
            DataDomain::De => &self.de,
        }
    }

    #[doc = "Returns the expected CE/DE security profile."]
    pub const fn security(&self) -> &SecurityProfileProof {
        &self.security
    }

    #[doc = "Returns a proof with replacement inodes for deterministic tests."]
    #[must_use]
    pub const fn with_inodes(mut self, inodes: DataInodes) -> Self {
        self.inodes = inodes;
        self
    }
}

#[doc = "Counts forbidden artifacts found while walking a copied tree."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeSafetyProof {
    symlinks: u64,
    device_nodes: u64,
    hardlinks: u64,
}

impl TreeSafetyProof {
    #[doc = "Constructs exact forbidden-artifact counters."]
    pub const fn new(symlinks: u64, device_nodes: u64, hardlinks: u64) -> Self {
        Self {
            symlinks,
            device_nodes,
            hardlinks,
        }
    }

    #[doc = "Returns a proof with no forbidden artifacts."]
    pub const fn clean() -> Self {
        Self::new(0, 0, 0)
    }

    pub(super) const fn first_violation(self) -> Option<UnsafeArtifact> {
        if self.symlinks != 0 {
            Some(UnsafeArtifact::Symlink)
        } else if self.device_nodes != 0 {
            Some(UnsafeArtifact::DeviceNode)
        } else if self.hardlinks != 0 {
            Some(UnsafeArtifact::Hardlink)
        } else {
            None
        }
    }
}

#[doc = "Forbidden copied-tree artifact category."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafeArtifact {
    #[doc = "Symbolic link."]
    Symlink,
    #[doc = "Character or block device node."]
    DeviceNode,
    #[doc = "File with multiple hard links."]
    Hardlink,
}
