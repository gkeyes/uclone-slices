use uclone_slot_runtime::android::{
    EmergencyGateLease, EmergencyGatePhase, GateLease, GateLeaseError, GateLeaseStore,
    StoredGateLease,
};
use uclone_slot_runtime::domain::PackageName;

#[derive(Debug, Default)]
pub(super) struct RecordingLeaseStore {
    emergency: Vec<EmergencyGateLease>,
    pub(super) persisted: Vec<GateLease>,
    pub(super) removed: Vec<PackageName>,
}

impl GateLeaseStore for RecordingLeaseStore {
    fn artifact_exists(&mut self, package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(!self.removed.contains(package)
            && (!self.persisted.is_empty() || !self.emergency.is_empty()))
    }

    fn load(&mut self, package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        if self.removed.contains(package) {
            return Ok(None);
        }
        Ok(self
            .persisted
            .last()
            .cloned()
            .map(StoredGateLease::Enrolled)
            .or_else(|| {
                self.emergency
                    .last()
                    .cloned()
                    .map(StoredGateLease::Emergency)
            }))
    }

    fn persist_emergency(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        self.emergency.push(lease.clone());
        Ok(())
    }

    fn mark_emergency_held(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        if lease.phase() != EmergencyGatePhase::Prepared {
            return Err(GateLeaseError::InvalidArtifact);
        }
        let Some(existing) = self.emergency.last_mut() else {
            return Err(GateLeaseError::InvalidArtifact);
        };
        if existing != lease {
            return Err(GateLeaseError::InvalidArtifact);
        }
        *existing = lease.held();
        Ok(())
    }

    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.persisted.push(lease.clone());
        Ok(())
    }

    fn confirm_enrollment(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.persisted.push(lease.clone());
        self.emergency.clear();
        Ok(())
    }

    fn retire(&mut self, expected: &GateLease) -> Result<(), GateLeaseError> {
        if self.persisted.last() != Some(expected) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.removed.push(expected.package_name().clone());
        Ok(())
    }
}
