use crate::android::PackageProbe;
use crate::domain::{GateSnapshot, ManagedPackage, PackageKey};
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::{CapabilitySnapshot, PackageState, ServiceError};

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_probe(&self) -> Result<CapabilitySnapshot, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        let unlocked = probe
            .user0_unlocked()
            .map_err(|_| ServiceError::UnsupportedDevice)?;
        let global = probe
            .mount_namespace_proof()
            .map_err(|_| ServiceError::UnsupportedDevice)?
            .is_global();
        Ok(CapabilitySnapshot::new(
            unlocked && global,
            unlocked,
            global,
            false,
        ))
    }

    pub(super) fn do_capture_gate(
        &mut self,
        key: &PackageKey,
    ) -> Result<GateSnapshot, ServiceError> {
        let package = self.ready_managed(key)?;
        self.runtime
            .capture_gate_snapshot(&package)
            .map_err(|_| ServiceError::Internal)
    }

    fn ready_managed(&self, key: &PackageKey) -> Result<ManagedPackage, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        match super::state::load(&self.stores, &mut *probe, key)? {
            PackageState::Ready(snapshot) => Ok(snapshot.managed().clone()),
            PackageState::Absent => Err(ServiceError::NotFound),
            PackageState::RecoveryRequired => Err(ServiceError::RecoveryRequired),
            PackageState::Quarantined => Err(ServiceError::Quarantined),
        }
    }
}
