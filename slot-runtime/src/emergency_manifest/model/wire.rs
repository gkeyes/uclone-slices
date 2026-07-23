use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::EmergencyManifestV1;
use crate::domain::BootId;
use crate::emergency_manifest::{
    ContainmentObligation, DiscoveryIntegrity, OverallDisposition, PackageContainment,
    RuntimeOwnerProof, SCHEMA_VERSION,
};

const LEGACY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize)]
struct ManifestRecordV3<'a> {
    schema_version: u32,
    boot_id: &'a BootId,
    generation: u64,
    discovery_integrity: DiscoveryIntegrity,
    runtime_owner_proof: Option<&'a RuntimeOwnerProof>,
    overall_disposition: OverallDisposition,
    packages: &'a [PackageContainment],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedManifestRecordV3 {
    schema_version: u32,
    boot_id: BootId,
    generation: u64,
    discovery_integrity: DiscoveryIntegrity,
    runtime_owner_proof: Option<RuntimeOwnerProof>,
    overall_disposition: OverallDisposition,
    packages: Vec<PackageContainment>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyManifestRecordV2 {
    schema_version: u32,
    boot_id: BootId,
    generation: u64,
    overall_disposition: OverallDisposition,
    packages: Vec<PackageContainment>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum VersionedManifestRecord {
    Current(OwnedManifestRecordV3),
    Legacy(LegacyManifestRecordV2),
}

impl Serialize for EmergencyManifestV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ManifestRecordV3 {
            schema_version: self.schema_version,
            boot_id: &self.boot_id,
            generation: self.generation,
            discovery_integrity: self.discovery_integrity,
            runtime_owner_proof: self.runtime_owner_proof.as_ref(),
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
        match VersionedManifestRecord::deserialize(deserializer)? {
            VersionedManifestRecord::Current(record) => {
                decode_current(record).map_err(serde::de::Error::custom)
            }
            VersionedManifestRecord::Legacy(record) => {
                upgrade_legacy(record).map_err(serde::de::Error::custom)
            }
        }
    }
}

fn decode_current(record: OwnedManifestRecordV3) -> Result<EmergencyManifestV1, String> {
    if record.schema_version != SCHEMA_VERSION {
        return Err("unsupported emergency manifest schema".to_owned());
    }
    let packages = record.packages.clone();
    let manifest = EmergencyManifestV1::with_context(
        record.boot_id,
        record.generation,
        record.discovery_integrity,
        record.runtime_owner_proof,
        record.packages,
    )
    .map_err(|error| error.to_string())?;
    verify_record_shape(&manifest, record.overall_disposition, &packages)?;
    Ok(manifest)
}

fn upgrade_legacy(record: LegacyManifestRecordV2) -> Result<EmergencyManifestV1, String> {
    if record.schema_version != LEGACY_SCHEMA_VERSION {
        return Err("unsupported emergency manifest schema".to_owned());
    }
    if record.overall_disposition == OverallDisposition::RuntimeOwned
        || record
            .packages
            .iter()
            .any(|entry| entry.obligation() == ContainmentObligation::RuntimeOwned)
    {
        return Err("legacy emergency manifest cannot contain RuntimeOwned".to_owned());
    }
    let packages = record.packages.clone();
    let honest = EmergencyManifestV1::new(
        record.boot_id.clone(),
        record.generation,
        record.packages.clone(),
    )
    .map_err(|error| error.to_string())?;
    verify_record_shape(&honest, record.overall_disposition, &packages)?;
    EmergencyManifestV1::with_context(
        record.boot_id,
        record.generation,
        DiscoveryIntegrity::Untrusted,
        None,
        record.packages,
    )
    .map_err(|error| error.to_string())
}

fn verify_record_shape(
    manifest: &EmergencyManifestV1,
    persisted_overall: OverallDisposition,
    persisted_packages: &[PackageContainment],
) -> Result<(), String> {
    if manifest.overall_disposition != persisted_overall {
        return Err("overall disposition does not match package obligations".to_owned());
    }
    if manifest.packages != persisted_packages {
        return Err("packages must be sorted by package name".to_owned());
    }
    Ok(())
}
