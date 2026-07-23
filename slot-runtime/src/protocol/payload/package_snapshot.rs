use serde::{Deserialize, Serialize};

use super::{PackageStatus, SlotsReport};
use crate::protocol::ProtocolError;
use crate::slot_metadata::SlotRecordState;

/// One coherent package status and slot-list confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshotReport {
    status: PackageStatus,
    slots: SlotsReport,
}

impl PackageSnapshotReport {
    /// Creates a cross-validated package snapshot.
    pub fn new(status: PackageStatus, slots: SlotsReport) -> Result<Self, ProtocolError> {
        let report = Self { status, slots };
        report.validate()?;
        Ok(report)
    }

    /// Returns the committed package status.
    pub const fn status(&self) -> &PackageStatus {
        &self.status
    }

    /// Returns Base and all visible extension slots from the same read.
    pub const fn slots(&self) -> &SlotsReport {
        &self.slots
    }

    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        if self.status.package() != self.slots.package() {
            return Err(ProtocolError::InvalidResponse);
        }
        let mut ids = std::collections::BTreeSet::new();
        if self
            .slots
            .slots()
            .iter()
            .any(|slot| !ids.insert(slot.slot()))
        {
            return Err(ProtocolError::InvalidResponse);
        }
        let mut active = self.slots.slots().iter().filter(|slot| slot.active());
        let Some(only_active) = active.next() else {
            return Err(ProtocolError::InvalidResponse);
        };
        if active.next().is_some()
            || only_active.slot() != self.status.slot()
            || only_active.state() != SlotRecordState::Ready
        {
            return Err(ProtocolError::InvalidResponse);
        }
        Ok(())
    }
}
