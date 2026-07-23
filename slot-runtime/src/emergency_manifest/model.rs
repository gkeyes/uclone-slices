use super::types::derive_overall;
use super::{
    ContainmentObligation, DiscoveryIntegrity, EmergencyManifestError, OverallDisposition,
    PackageContainment, RuntimeOwnerProof, SCHEMA_VERSION,
};
use crate::domain::{BootId, PackageName};

mod transition;
mod validation;
mod wire;

pub(super) const MAX_PACKAGES: usize = 512;

/// Versioned, boot-scoped emergency containment manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyManifestV1 {
    schema_version: u32,
    boot_id: BootId,
    generation: u64,
    discovery_integrity: DiscoveryIntegrity,
    runtime_owner_proof: Option<RuntimeOwnerProof>,
    overall_disposition: OverallDisposition,
    packages: Vec<PackageContainment>,
}

impl EmergencyManifestV1 {
    /// Builds a manifest and derives its overall disposition from package entries.
    pub fn new(
        boot_id: BootId,
        generation: u64,
        packages: Vec<PackageContainment>,
    ) -> Result<Self, EmergencyManifestError> {
        Self::with_context(
            boot_id,
            generation,
            DiscoveryIntegrity::Complete,
            None,
            packages,
        )
    }

    /// Builds a manifest from explicit discovery integrity and Runtime ownership evidence.
    pub fn with_context(
        boot_id: BootId,
        generation: u64,
        discovery_integrity: DiscoveryIntegrity,
        runtime_owner_proof: Option<RuntimeOwnerProof>,
        mut packages: Vec<PackageContainment>,
    ) -> Result<Self, EmergencyManifestError> {
        if generation == 0 {
            return Err(EmergencyManifestError::InvalidManifest(
                "generation must be non-zero".to_owned(),
            ));
        }
        if packages.len() > MAX_PACKAGES {
            return Err(EmergencyManifestError::BoundExceeded("package entries"));
        }
        packages.sort_by(|left, right| left.package().cmp(right.package()));
        for pair in packages.windows(2) {
            let [previous, next] = pair else {
                return Err(EmergencyManifestError::Corrupt(
                    "invalid package ordering window".to_owned(),
                ));
            };
            if previous.package() == next.package() {
                return Err(EmergencyManifestError::InvalidManifest(format!(
                    "duplicate package {}",
                    previous.package()
                )));
            }
        }
        if packages.iter().any(|entry| entry.enrollment_epoch() == 0) {
            return Err(EmergencyManifestError::InvalidManifest(
                "enrollment epoch must be non-zero".to_owned(),
            ));
        }
        let overall_disposition = derive_overall(discovery_integrity, &packages);
        let manifest = Self {
            schema_version: SCHEMA_VERSION,
            boot_id,
            generation,
            discovery_integrity,
            runtime_owner_proof,
            overall_disposition,
            packages,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Returns the schema version of this manifest.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the boot identifier pinned by this manifest.
    pub const fn boot_id(&self) -> &BootId {
        &self.boot_id
    }

    /// Returns the one-based manifest generation.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns whether package discovery was complete and trustworthy.
    pub const fn discovery_integrity(&self) -> DiscoveryIntegrity {
        self.discovery_integrity
    }

    /// Returns the exact Runtime incarnation that consumers must revalidate live.
    pub const fn runtime_owner_proof(&self) -> Option<&RuntimeOwnerProof> {
        self.runtime_owner_proof.as_ref()
    }

    /// Returns the disposition derived from all package obligations.
    pub const fn overall_disposition(&self) -> OverallDisposition {
        self.overall_disposition
    }

    /// Returns package entries in deterministic `PackageName` order.
    pub fn packages(&self) -> &[PackageContainment] {
        &self.packages
    }

    /// Returns one package's obligation when it is present in the manifest.
    pub fn obligation_for(&self, package: &PackageName) -> Option<ContainmentObligation> {
        self.packages
            .binary_search_by(|entry| entry.package().cmp(package))
            .ok()
            .and_then(|index| self.packages.get(index))
            .map(PackageContainment::obligation)
    }
}
