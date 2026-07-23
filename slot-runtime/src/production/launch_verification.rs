use crate::android::PackageProbe;
use crate::domain::{ManagedPackage, SlotView};
use crate::lifecycle::{GuardDecision, PackageLifecycleGuard};
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::ServiceError;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_verify_current_view_for_launch(
        &mut self,
        package: &ManagedPackage,
        target: &SlotView,
    ) -> Result<(), ServiceError> {
        let result = self.verify_current_view(package, target);
        match result {
            Ok(()) => Ok(()),
            Err(class) => {
                self.do_contain(package, class)
                    .map_err(|_| ServiceError::RecoveryRequired)?;
                Err(class)
            }
        }
    }

    fn verify_current_view(
        &mut self,
        package: &ManagedPackage,
        target: &SlotView,
    ) -> Result<(), ServiceError> {
        let snapshot = self
            .runtime
            .capture_gate_snapshot(package)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.runtime
            .acquire_gate(package)
            .and_then(|()| self.runtime.verify_gate_held(package))
            .and_then(|()| self.runtime.quiesce_processes(package))
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let observed = self
            .runtime
            .observe_package(package)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        match PackageLifecycleGuard::assess(package, &observed) {
            GuardDecision::AllowBase | GuardDecision::AllowSlot => {}
            GuardDecision::Quarantine => return Err(ServiceError::Quarantined),
            GuardDecision::RecoveryRequired(_)
            | GuardDecision::RequireSafeUpdateWindow
            | GuardDecision::AllowUpdateVerification => {
                return Err(ServiceError::RecoveryRequired);
            }
        }
        self.runtime
            .verify_slot_view(package, target)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.runtime
            .restore_gate(package, snapshot)
            .and_then(|()| self.runtime.retire_gate_lease(package))
            .map_err(|_| ServiceError::RecoveryRequired)
    }
}
