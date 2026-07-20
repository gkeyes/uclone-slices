use crate::catalog::SecurityProfileProof;
use crate::domain::{AppIdentity, DataInodes, ManagedPackage, PackageKey, SlotId};

use super::{MaterializationPaths, SlotMaterializationProof};

#[doc = "Typed, path-safe input for immutable Catalog publication."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationResult {
    package_key: PackageKey,
    slot_id: SlotId,
    inodes: DataInodes,
    enrolled_identity: AppIdentity,
    created_version_code: u64,
    security: SecurityProfileProof,
    paths: MaterializationPaths,
}

impl MaterializationResult {
    pub(crate) fn from_proof(
        package: &ManagedPackage,
        slot_id: SlotId,
        proof: &SlotMaterializationProof,
        paths: MaterializationPaths,
    ) -> Self {
        Self {
            package_key: PackageKey::new(package.package_name().clone(), package.user_id()),
            slot_id,
            inodes: proof.inodes(),
            enrolled_identity: package.identity().clone(),
            created_version_code: package.identity().version_code(),
            security: proof.security().clone(),
            paths,
        }
    }

    #[doc = "Returns the package/user catalog key."]
    pub const fn package_key(&self) -> &PackageKey {
        &self.package_key
    }

    #[doc = "Returns the fixed Preview slot identifier."]
    pub const fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    #[doc = "Returns the published independent CE/DE inodes."]
    pub const fn inodes(&self) -> DataInodes {
        self.inodes
    }

    #[doc = "Returns the immutable enrollment identity."]
    pub const fn enrolled_identity(&self) -> &AppIdentity {
        &self.enrolled_identity
    }

    #[doc = "Returns the installed version at materialization time."]
    pub const fn created_version_code(&self) -> u64 {
        self.created_version_code
    }

    #[doc = "Returns the verified ready-path security profile."]
    pub const fn security_profile(&self) -> &SecurityProfileProof {
        &self.security
    }

    #[doc = "Returns only coordinator-derived fixed paths."]
    pub const fn paths(&self) -> &MaterializationPaths {
        &self.paths
    }
}
