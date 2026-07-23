use serde::{Deserialize, Serialize, de::Deserializer};

use super::ProtocolError;

mod app_types;
mod outcome;
mod package_snapshot;
mod probe;
mod types;

pub use app_types::{
    ManagedAppSummary, ManagedAppsReport, PackageInspectionReport, RecoveryTargetsReport,
    SlotSummary, SlotsReport,
};
pub use outcome::ReconcileOutcome;
pub use package_snapshot::PackageSnapshotReport;
pub use probe::ProbeReport;
pub use types::{
    Ack, AckOperation, LaunchResult, LaunchStatus, PackageStatus, ReconcileReport, SwitchResult,
};

/// Fixed success payload variants; no arbitrary JSON is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ResponsePayload {
    /// Runtime and device capability report.
    ProbeReport(ProbeReport),
    /// Installed package compatibility and identity report.
    PackageInspection(PackageInspectionReport),
    /// All durable managed package rows.
    ManagedApps(ManagedAppsReport),
    /// Fixed-root packages eligible for recovery-only Base rescue.
    RecoveryTargets(RecoveryTargetsReport),
    /// Base and non-base slot rows for one package.
    Slots(SlotsReport),
    /// Current package view and lifecycle state.
    PackageStatus(PackageStatus),
    /// Coherent package status and slot rows used before an explicit launch.
    PackageSnapshot(PackageSnapshotReport),
    /// Verified result of a slot switch.
    SwitchResult(SwitchResult),
    /// Result of an explicit launch for an already-confirmed slot.
    LaunchResult(LaunchResult),
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
    PackageInspection(PackageInspectionReport),
    ManagedApps(ManagedAppsReport),
    RecoveryTargets(RecoveryTargetsReport),
    Slots(SlotsReport),
    PackageStatus(PackageStatus),
    PackageSnapshot(PackageSnapshotReport),
    SwitchResult(SwitchResult),
    LaunchResult(LaunchResult),
    ReconcileReport(ReconcileReport),
    Ack(Ack),
}

impl<'de> Deserialize<'de> for ResponsePayload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireResponsePayload::deserialize(deserializer)?;
        let payload = match wire {
            WireResponsePayload::ProbeReport(value) => Self::ProbeReport(value),
            WireResponsePayload::PackageInspection(value) => Self::PackageInspection(value),
            WireResponsePayload::ManagedApps(value) => Self::ManagedApps(value),
            WireResponsePayload::RecoveryTargets(value) => Self::RecoveryTargets(value),
            WireResponsePayload::Slots(value) => Self::Slots(value),
            WireResponsePayload::PackageStatus(value) => Self::PackageStatus(value),
            WireResponsePayload::PackageSnapshot(value) => Self::PackageSnapshot(value),
            WireResponsePayload::SwitchResult(value) => Self::SwitchResult(value),
            WireResponsePayload::LaunchResult(value) => Self::LaunchResult(value),
            WireResponsePayload::ReconcileReport(value) => Self::ReconcileReport(value),
            WireResponsePayload::Ack(value) => Self::Ack(value),
        };
        payload.validate().map_err(serde::de::Error::custom)?;
        Ok(payload)
    }
}

impl ResponsePayload {
    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        if matches!(
            self,
            Self::RecoveryTargets(report)
                if report.targets().len() > crate::service::MAX_RECOVERY_TARGETS
        ) {
            return Err(ProtocolError::InvalidResponse);
        }
        if let Self::PackageSnapshot(report) = self {
            report.validate()?;
        }
        let size = serde_json::to_vec(self)?.len();
        if size < crate::protocol::MAX_FRAME_SIZE {
            Ok(())
        } else {
            Err(ProtocolError::FrameTooLarge { size })
        }
    }
}
