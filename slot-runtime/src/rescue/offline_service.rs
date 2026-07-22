use crate::domain::{GateSnapshot, ManagedPackage, PackageKey, SlotView};
use crate::reconcile::{
    NativeBaseRecoveryBackend, ReconcileOutcome, ReconcileReason, RecoveryBackend,
};
use crate::service::{
    CapabilitySnapshot, EnrollmentPublicationError, PackageState, ServiceError, ServicePlatform,
    SwitchExecution,
};

use super::{
    OfflineRescuePlatform, RescueExecution, RescueFaultInjector, RescueMetadataSource,
    RescueStartup,
};

impl<B, T, F> ServicePlatform for OfflineRescuePlatform<B, T, F>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    T: RescueMetadataSource,
    F: RescueFaultInjector,
{
    fn probe(&self) -> Result<CapabilitySnapshot, ServiceError> {
        Ok(CapabilitySnapshot::new(false, false, true))
    }

    fn package_state(&self, _key: &PackageKey) -> Result<PackageState, ServiceError> {
        Ok(PackageState::RecoveryRequired)
    }

    fn capture_gate(&mut self, _key: &PackageKey) -> Result<GateSnapshot, ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn begin_enrollment_attempt(
        &mut self,
        _key: &PackageKey,
    ) -> Result<GateSnapshot, ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn abort_enrollment_attempt(
        &mut self,
        _key: &PackageKey,
        _snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn hold_gate(&mut self, _key: &PackageKey) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn quiesce(&mut self, _key: &PackageKey) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn enroll_atomically(
        &mut self,
        _key: &PackageKey,
        _accept_direct_boot_conditional: bool,
    ) -> Result<ManagedPackage, EnrollmentPublicationError> {
        Err(EnrollmentPublicationError::PublicationAmbiguous)
    }

    fn prove_base(&mut self, _package: &ManagedPackage) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn restore_gate(
        &mut self,
        _package: &ManagedPackage,
        _snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn mark_recovery_required(&mut self, _package: &ManagedPackage) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn mark_enrollment_failure(
        &mut self,
        _key: &PackageKey,
        _class: ServiceError,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn contain_failure(
        &mut self,
        _package: &ManagedPackage,
        _class: ServiceError,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn materialize_slot(
        &mut self,
        _package: &ManagedPackage,
        _slot: &crate::domain::SlotId,
        _seed_mode: crate::slot_metadata::SlotSeedMode,
    ) -> Result<SlotView, ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn switch_view(
        &mut self,
        _package: &ManagedPackage,
        _target: &SlotView,
        _prepared_gate: Option<GateSnapshot>,
    ) -> Result<SwitchExecution, ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    fn reconcile_two_phase(&mut self, key: &PackageKey) -> Result<ReconcileOutcome, ServiceError> {
        Ok(match self.reconcile_startup(key) {
            RescueStartup::BaseRetired => ReconcileOutcome::RestoredBase,
            RescueStartup::Quarantined => ReconcileOutcome::Quarantined,
            RescueStartup::ContainmentFailed => {
                return Err(ServiceError::Internal);
            }
            RescueStartup::OpenOrdinary | RescueStartup::RecoveryRequired => {
                ReconcileOutcome::RecoveryRequired(ReconcileReason::JournalMetadata)
            }
        })
    }

    fn rescue_to_base(&mut self, key: &PackageKey) -> Result<RescueExecution, ServiceError> {
        Ok(Self::rescue_to_base(self, key))
    }
}
