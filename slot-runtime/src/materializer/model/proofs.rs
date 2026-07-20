use std::path::{Path, PathBuf};

use crate::catalog::SecurityProfileProof;
use crate::domain::{DataInodes, ManagedPackage, SlotId};
use crate::layout::RuntimeLayout;

use super::{
    ContentDigest, ContentProof, DataDomain, DirectoryAnchor, MaterializationProofError,
    TreeSafetyProof,
};

#[doc = "Typed receipt for one copied data domain."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainCopyProof {
    domain: DataDomain,
    content: ContentDigest,
    safety: TreeSafetyProof,
}

impl DomainCopyProof {
    #[doc = "Validates a copy receipt without accepting a source or destination path."]
    pub fn new(
        domain: DataDomain,
        content: &str,
        safety: TreeSafetyProof,
    ) -> Result<Self, MaterializationProofError> {
        Ok(Self {
            domain,
            content: ContentDigest::parse(content)?,
            safety,
        })
    }

    #[doc = "Returns the copied data domain."]
    pub const fn domain(&self) -> DataDomain {
        self.domain
    }

    #[doc = "Returns the copied content digest."]
    pub const fn content(&self) -> &ContentDigest {
        &self.content
    }

    #[doc = "Returns forbidden-artifact counters for the copied tree."]
    pub const fn safety(&self) -> TreeSafetyProof {
        self.safety
    }
}

#[doc = "Ready and staging paths derived only from package and slot identifiers."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationPaths {
    ready_ce: PathBuf,
    ready_de: PathBuf,
    staging_ce: PathBuf,
    staging_de: PathBuf,
}

impl MaterializationPaths {
    pub(crate) fn derive(package: &ManagedPackage, slot: &SlotId) -> Self {
        let ready = RuntimeLayout::slot_paths(package.package_name(), slot);
        Self {
            ready_ce: ready.ce().to_path_buf(),
            ready_de: ready.de().to_path_buf(),
            staging_ce: ready
                .ce()
                .with_file_name(format!(".{}.staging", slot.as_str())),
            staging_de: ready
                .de()
                .with_file_name(format!(".{}.staging", slot.as_str())),
        }
    }

    #[doc = "Returns the fixed CE ready path."]
    pub fn ready_ce(&self) -> &Path {
        &self.ready_ce
    }

    #[doc = "Returns the fixed DE ready path."]
    pub fn ready_de(&self) -> &Path {
        &self.ready_de
    }

    #[doc = "Returns the fixed CE staging path."]
    pub fn staging_ce(&self) -> &Path {
        &self.staging_ce
    }

    #[doc = "Returns the fixed DE staging path."]
    pub fn staging_de(&self) -> &Path {
        &self.staging_de
    }
}

#[doc = "Complete evidence for paired CE and DE staging or ready directories."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotMaterializationProof {
    inodes: DataInodes,
    ce: DirectoryAnchor,
    de: DirectoryAnchor,
    contents: ContentProof,
    security: SecurityProfileProof,
    ce_safety: TreeSafetyProof,
    de_safety: TreeSafetyProof,
    complete: bool,
}

impl SlotMaterializationProof {
    #[doc = "Constructs a proof only when directory and paired content digests agree."]
    #[allow(
        clippy::too_many_arguments,
        reason = "all independently observed security proof fields remain explicit"
    )]
    pub fn new(
        inodes: DataInodes,
        ce: DirectoryAnchor,
        de: DirectoryAnchor,
        contents: ContentProof,
        security: SecurityProfileProof,
        ce_safety: TreeSafetyProof,
        de_safety: TreeSafetyProof,
        complete: bool,
    ) -> Result<Self, MaterializationProofError> {
        if ce.content() != contents.ce() || de.content() != contents.de() {
            return Err(MaterializationProofError::InconsistentContent);
        }
        Ok(Self {
            inodes,
            ce,
            de,
            contents,
            security,
            ce_safety,
            de_safety,
            complete,
        })
    }

    #[doc = "Returns the independent staging or ready inode pair."]
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

    #[doc = "Returns the paired content proof."]
    pub const fn contents(&self) -> &ContentProof {
        &self.contents
    }

    #[doc = "Returns the observed security profile."]
    pub const fn security(&self) -> &SecurityProfileProof {
        &self.security
    }

    #[doc = "Returns one domain tree-safety proof."]
    pub const fn safety(&self, domain: DataDomain) -> TreeSafetyProof {
        match domain {
            DataDomain::Ce => self.ce_safety,
            DataDomain::De => self.de_safety,
        }
    }

    #[doc = "Returns whether both staging domains were fully enumerated."]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    #[doc = "Returns a proof with replacement security for deterministic tests."]
    #[must_use]
    pub fn with_security(mut self, security: SecurityProfileProof) -> Self {
        self.security = security;
        self
    }

    #[doc = "Returns a proof with replacement filesystem device identifiers."]
    pub fn with_devices(mut self, ce: u64, de: u64) -> Result<Self, MaterializationProofError> {
        self.ce = DirectoryAnchor::new(ce, self.contents.ce().as_str())?;
        self.de = DirectoryAnchor::new(de, self.contents.de().as_str())?;
        Ok(self)
    }
}
