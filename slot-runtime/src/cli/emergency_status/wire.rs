use serde::Serialize;

use crate::domain::BootId;
use crate::emergency_manifest::{DiscoveryIntegrity, EmergencyManifestRecord, OverallDisposition};
use crate::protocol::RUNTIME_BUILD_ID;

use super::EmergencyStatusError;

const RESPONSE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OwnerVerdict {
    Valid,
    NotRequired,
    Missing,
    Invalid,
    Unavailable,
}

#[derive(Debug, Serialize)]
pub(super) struct PackageEntry {
    package: String,
    enrollment_epoch: u64,
    obligation: crate::emergency_manifest::ContainmentObligation,
}

#[derive(Debug, Serialize)]
pub(super) struct Page {
    cursor: usize,
    next: Option<usize>,
    total: usize,
    entries: Vec<PackageEntry>,
}

#[derive(Debug)]
pub(super) struct EmergencySnapshot {
    boot_id: String,
    generation: u64,
    raw_sha256: String,
    discovery_integrity: DiscoveryIntegrity,
    overall_disposition: OverallDisposition,
    owner_verdict: OwnerVerdict,
    page: Page,
}

impl EmergencySnapshot {
    pub(super) fn new(
        boot_id: &BootId,
        record: &EmergencyManifestRecord,
        owner_verdict: OwnerVerdict,
        cursor: usize,
        end: usize,
    ) -> Self {
        let manifest = record.manifest();
        let entries = manifest
            .packages()
            .iter()
            .skip(cursor)
            .take(end.saturating_sub(cursor))
            .map(|entry| PackageEntry {
                package: entry.package().as_str().to_owned(),
                enrollment_epoch: entry.enrollment_epoch(),
                obligation: entry.obligation(),
            })
            .collect();
        let total = manifest.packages().len();
        let next = (end < total).then_some(end);
        Self {
            boot_id: boot_id.as_str().to_owned(),
            generation: manifest.generation(),
            raw_sha256: record.raw_sha256().to_owned(),
            discovery_integrity: manifest.discovery_integrity(),
            overall_disposition: manifest.overall_disposition(),
            owner_verdict,
            page: Page {
                cursor,
                next,
                total,
                entries,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct EmergencyStatusFrame {
    schema_version: u32,
    request_id: String,
    status: &'static str,
    build_id: &'static str,
    boot_id: Option<String>,
    generation: Option<u64>,
    raw_sha256: Option<String>,
    discovery_integrity: DiscoveryIntegrity,
    overall_disposition: OverallDisposition,
    owner_verdict: OwnerVerdict,
    page: Page,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'static str>,
}

impl EmergencyStatusFrame {
    pub(super) fn success(request_id: &str, snapshot: EmergencySnapshot) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            request_id: request_id.to_owned(),
            status: "ok",
            build_id: RUNTIME_BUILD_ID,
            boot_id: Some(snapshot.boot_id),
            generation: Some(snapshot.generation),
            raw_sha256: Some(snapshot.raw_sha256),
            discovery_integrity: snapshot.discovery_integrity,
            overall_disposition: snapshot.overall_disposition,
            owner_verdict: snapshot.owner_verdict,
            page: snapshot.page,
            error_code: None,
        }
    }

    pub(super) fn failure(request_id: &str, error: &EmergencyStatusError) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            request_id: request_id.to_owned(),
            status: "error",
            build_id: RUNTIME_BUILD_ID,
            boot_id: None,
            generation: None,
            raw_sha256: None,
            discovery_integrity: DiscoveryIntegrity::Untrusted,
            overall_disposition: OverallDisposition::HeldRecovery,
            owner_verdict: owner_failure(error),
            page: Page {
                cursor: 0,
                next: None,
                total: 0,
                entries: Vec::new(),
            },
            error_code: Some(error_code(error)),
        }
    }
}

const fn owner_failure(error: &EmergencyStatusError) -> OwnerVerdict {
    match error {
        EmergencyStatusError::OwnerMissing => OwnerVerdict::Missing,
        EmergencyStatusError::OwnerInvalid(_) => OwnerVerdict::Invalid,
        _ => OwnerVerdict::Unavailable,
    }
}

const fn error_code(error: &EmergencyStatusError) -> &'static str {
    match error {
        EmergencyStatusError::Manifest(
            crate::emergency_manifest::EmergencyManifestError::MissingRoot { .. },
        ) => "manifest_root_missing",
        EmergencyStatusError::Manifest(
            crate::emergency_manifest::EmergencyManifestError::MissingManifest { .. },
        ) => "manifest_missing",
        EmergencyStatusError::Manifest(_) => "manifest_invalid",
        EmergencyStatusError::BootProbe(_) | EmergencyStatusError::InvalidBoot => {
            "boot_probe_failed"
        }
        EmergencyStatusError::StaleBoot { .. } => "manifest_stale_boot",
        EmergencyStatusError::OwnerMissing => "owner_missing",
        EmergencyStatusError::OwnerInvalid(_) => "owner_invalid",
        EmergencyStatusError::OwnerProbe(_) => "owner_probe_failed",
        EmergencyStatusError::CursorOutOfRange { .. } => "cursor_out_of_range",
        EmergencyStatusError::Encode | EmergencyStatusError::FrameTooLarge(_) => "response_invalid",
    }
}
