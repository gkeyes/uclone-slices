use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::types::derive_overall;
use super::{
    ContainmentObligation, EmergencyManifestError, OverallDisposition, PackageContainment,
    SCHEMA_VERSION,
};
use crate::domain::{BootId, PackageName};

mod transition;

const MAX_PACKAGES: usize = 512;

/// Versioned, boot-scoped emergency containment manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyManifestV1 {
    schema_version: u32,
    boot_id: BootId,
    generation: u64,
    overall_disposition: OverallDisposition,
    packages: Vec<PackageContainment>,
}

#[derive(Debug, Serialize)]
struct ManifestRecord<'a> {
    schema_version: u32,
    boot_id: &'a BootId,
    generation: u64,
    overall_disposition: OverallDisposition,
    packages: &'a [PackageContainment],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedManifestRecord {
    schema_version: u32,
    boot_id: BootId,
    generation: u64,
    overall_disposition: OverallDisposition,
    packages: Vec<PackageContainment>,
}

impl EmergencyManifestV1 {
    /// Builds a manifest and derives its overall disposition from package entries.
    pub fn new(
        boot_id: BootId,
        generation: u64,
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
        let overall_disposition = derive_overall(&packages);
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            boot_id,
            generation,
            overall_disposition,
            packages,
        })
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

    pub(crate) fn validate(&self) -> Result<(), EmergencyManifestError> {
        if self.schema_version != SCHEMA_VERSION || self.generation == 0 {
            return Err(EmergencyManifestError::InvalidManifest(
                "unsupported schema or zero generation".to_owned(),
            ));
        }
        if self.packages.len() > MAX_PACKAGES {
            return Err(EmergencyManifestError::BoundExceeded("package entries"));
        }
        for pair in self.packages.windows(2) {
            let [previous, next] = pair else {
                return Err(EmergencyManifestError::Corrupt(
                    "invalid package ordering window".to_owned(),
                ));
            };
            if previous.package() >= next.package() {
                return Err(EmergencyManifestError::InvalidManifest(
                    "packages must be unique and sorted".to_owned(),
                ));
            }
        }
        if self
            .packages
            .iter()
            .any(|entry| entry.enrollment_epoch() == 0)
        {
            return Err(EmergencyManifestError::InvalidManifest(
                "enrollment epoch must be non-zero".to_owned(),
            ));
        }
        if derive_overall(&self.packages) != self.overall_disposition {
            return Err(EmergencyManifestError::InvalidManifest(
                "overall disposition does not match package obligations".to_owned(),
            ));
        }
        Ok(())
    }
}

impl Serialize for EmergencyManifestV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ManifestRecord {
            schema_version: self.schema_version,
            boot_id: &self.boot_id,
            generation: self.generation,
            overall_disposition: self.overall_disposition,
            packages: &self.packages,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EmergencyManifestV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let record = OwnedManifestRecord::deserialize(deserializer)?;
        let packages = record.packages.clone();
        let manifest = Self::new(record.boot_id, record.generation, record.packages)
            .map_err(serde::de::Error::custom)?;
        if record.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(
                "unsupported emergency manifest schema",
            ));
        }
        if manifest.overall_disposition != record.overall_disposition {
            return Err(serde::de::Error::custom(
                "overall disposition does not match package obligations",
            ));
        }
        if manifest.packages != packages {
            return Err(serde::de::Error::custom(
                "packages must be sorted by package name",
            ));
        }
        Ok(manifest)
    }
}
