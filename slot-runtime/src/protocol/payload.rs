use serde::{Deserialize, Serialize, de::Deserializer};

use super::{ALLOWED_PACKAGE, ProtocolError};
use crate::domain::PackageName;

mod outcome;
mod types;

pub use outcome::ReconcileOutcome;
pub use types::{Ack, AckOperation, PackageStatus, ProbeReport, ReconcileReport, SwitchResult};

/// Fixed success payload variants; no arbitrary JSON is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ResponsePayload {
    /// Runtime and device capability report.
    ProbeReport(ProbeReport),
    /// Current package view and lifecycle state.
    PackageStatus(PackageStatus),
    /// Verified result of a slot switch.
    SwitchResult(SwitchResult),
    /// Result of one reconciliation pass.
    ReconcileReport(ReconcileReport),
    /// A command that has no richer result was durably acknowledged.
    Ack(Ack),
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum WireResponsePayload {
    ProbeReport(ProbeReport),
    PackageStatus(PackageStatus),
    SwitchResult(SwitchResult),
    ReconcileReport(ReconcileReport),
    Ack(Ack),
}

impl<'de> Deserialize<'de> for ResponsePayload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireResponsePayload::deserialize(deserializer)?;
        let payload = match wire {
            WireResponsePayload::ProbeReport(value) => Self::ProbeReport(value),
            WireResponsePayload::PackageStatus(value) => Self::PackageStatus(value),
            WireResponsePayload::SwitchResult(value) => Self::SwitchResult(value),
            WireResponsePayload::ReconcileReport(value) => Self::ReconcileReport(value),
            WireResponsePayload::Ack(value) => Self::Ack(value),
        };
        payload.validate().map_err(serde::de::Error::custom)?;
        Ok(payload)
    }
}

impl ResponsePayload {
    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        let package = match self {
            Self::ProbeReport(report) => report.package(),
            Self::PackageStatus(status) => status.package(),
            Self::SwitchResult(result) => result.package(),
            Self::ReconcileReport(report) => report.package(),
            Self::Ack(_) => return Ok(()),
        };
        validate_package(package)
    }
}

fn validate_package(package: &PackageName) -> Result<(), ProtocolError> {
    if package.as_str() == ALLOWED_PACKAGE {
        Ok(())
    } else {
        Err(ProtocolError::PackageNotAllowed(package.to_string()))
    }
}
