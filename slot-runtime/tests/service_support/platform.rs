use uclone_slot_runtime::domain::{GateSnapshot, ManagedPackage, PackageKey, SlotView};
use uclone_slot_runtime::reconcile::ReconcileOutcome;
use uclone_slot_runtime::service::{
    CapabilitySnapshot, EnrollmentPublicationError, ManagedAppInfo, PackageInspection,
    PackageSnapshot, PackageState, RescueExecution, ServiceError, ServicePlatform, SlotInfo,
    SwitchExecution,
};
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotSeedMode};

use super::fixtures::{
    assert_key, base_inodes, managed_base, original_gate, preview_view, ready_base,
};
use super::{Call, FailurePoint, FakePlatform};

impl ServicePlatform for FakePlatform {
    fn probe(&self) -> Result<CapabilitySnapshot, ServiceError> {
        self.record(Call::Probe);
        self.fail(FailurePoint::Probe)?;
        Ok(CapabilitySnapshot::new(true, true, true))
    }

    fn inspect_package(&self, key: &PackageKey) -> Result<PackageInspection, ServiceError> {
        self.inspect_fake(key)
    }

    fn list_managed_apps(&self) -> Result<Vec<ManagedAppInfo>, ServiceError> {
        self.list_managed_fake()
    }

    fn list_slots(&self, key: &PackageKey) -> Result<Vec<SlotInfo>, ServiceError> {
        self.list_slots_fake(key)
    }

    fn create_slot(
        &mut self,
        key: &PackageKey,
        _: SlotDisplayName,
        _: SlotSeedMode,
    ) -> Result<SwitchExecution, ServiceError> {
        self.create_fake(key)
    }

    fn rename_slot(
        &mut self,
        key: &PackageKey,
        _: &uclone_slot_runtime::domain::SlotId,
        _: SlotDisplayName,
    ) -> Result<(), ServiceError> {
        self.rename_fake(key)
    }

    fn delete_slot(
        &mut self,
        key: &PackageKey,
        _: &uclone_slot_runtime::domain::SlotId,
    ) -> Result<(), ServiceError> {
        self.delete_fake(key)
    }

    fn package_state(&self, key: &PackageKey) -> Result<PackageState, ServiceError> {
        assert_key(key);
        self.record(Call::State);
        self.fail(FailurePoint::State)?;
        Ok(self.state.borrow().clone())
    }

    fn capture_gate(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError> {
        assert_key(key);
        self.record(Call::CaptureGate);
        self.fail(FailurePoint::CaptureGate)?;
        Ok(original_gate())
    }

    fn begin_enrollment_attempt(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError> {
        assert_key(key);
        self.record(Call::BeginEnrollment);
        self.fail(FailurePoint::BeginEnrollment)?;
        self.enrollment_anchor.replace(true);
        Ok(original_gate())
    }

    fn abort_enrollment_attempt(
        &mut self,
        key: &PackageKey,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        assert_key(key);
        assert_eq!(snapshot, original_gate());
        self.record(Call::AbortEnrollment);
        self.fail(FailurePoint::AbortEnrollment)?;
        self.enrollment_anchor.replace(false);
        Ok(())
    }

    fn hold_gate(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        assert_key(key);
        self.record(Call::HoldGate);
        self.fail(FailurePoint::HoldGate)
    }

    fn quiesce(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        assert_key(key);
        self.record(Call::Quiesce);
        self.fail(FailurePoint::Quiesce)
    }

    fn enroll_atomically(
        &mut self,
        key: &PackageKey,
        accept_direct_boot_conditional: bool,
    ) -> Result<ManagedPackage, EnrollmentPublicationError> {
        assert_key(key);
        self.record(Call::Enroll(accept_direct_boot_conditional));
        self.fail(FailurePoint::Enroll)
            .map_err(EnrollmentPublicationError::Unpublished)?;
        if self.fail(FailurePoint::EnrollAmbiguous).is_err() {
            return Err(EnrollmentPublicationError::PublicationAmbiguous);
        }
        let managed = managed_base();
        self.state.replace(ready_base());
        Ok(managed)
    }

    fn prove_base(&mut self, package: &ManagedPackage) -> Result<(), ServiceError> {
        assert_eq!(package.base_inodes(), base_inodes());
        self.record(Call::ProveBase);
        self.fail(FailurePoint::ProveBase)
    }

    fn restore_gate(
        &mut self,
        _: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        assert_eq!(snapshot, original_gate());
        self.record(Call::RestoreGate);
        self.fail(FailurePoint::RestoreGate)
    }

    fn retire_gate_lease(&mut self, _: &ManagedPackage) -> Result<(), ServiceError> {
        self.record(Call::RetireGate);
        self.fail(FailurePoint::RetireGate)?;
        self.enrollment_anchor.replace(false);
        Ok(())
    }

    fn mark_recovery_required(&mut self, _: &ManagedPackage) -> Result<(), ServiceError> {
        self.record(Call::MarkRecovery);
        self.state.replace(PackageState::RecoveryRequired);
        self.fail(FailurePoint::MarkRecovery)
    }

    fn mark_enrollment_failure(
        &mut self,
        _: &PackageKey,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.record(Call::MarkEnrollment(class));
        self.enrollment_anchor.replace(true);
        self.fail(FailurePoint::MarkEnrollment)
    }

    fn contain_failure(
        &mut self,
        _: &ManagedPackage,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.record(Call::Contain(class));
        self.state.replace(match class {
            ServiceError::Quarantined => PackageState::Quarantined,
            ServiceError::InvalidRequest
            | ServiceError::PackageNotAllowed
            | ServiceError::DirectBootConfirmationRequired
            | ServiceError::NotFound
            | ServiceError::Conflict
            | ServiceError::RecoveryRequired
            | ServiceError::Busy
            | ServiceError::UserLocked
            | ServiceError::UnsupportedDevice
            | ServiceError::Internal => PackageState::RecoveryRequired,
        });
        self.fail(FailurePoint::Contain)
    }

    fn materialize_slot(
        &mut self,
        _: &ManagedPackage,
        _slot: &uclone_slot_runtime::domain::SlotId,
        _seed_mode: uclone_slot_runtime::slot_metadata::SlotSeedMode,
    ) -> Result<SlotView, ServiceError> {
        self.record(Call::Materialize);
        self.fail(FailurePoint::Materialize)?;
        let target = preview_view();
        let PackageState::Ready(snapshot) = self.state.borrow().clone() else {
            return Err(ServiceError::RecoveryRequired);
        };
        self.state
            .replace(PackageState::Ready(Box::new(PackageSnapshot::new(
                snapshot.managed().clone(),
                vec![target.clone()],
                snapshot.gate(),
            ))));
        Ok(target)
    }

    fn switch_view(
        &mut self,
        _: &ManagedPackage,
        target: &SlotView,
        prepared_gate: Option<GateSnapshot>,
    ) -> Result<SwitchExecution, ServiceError> {
        self.record(Call::Switch {
            slot: target.slot_id().clone(),
            prepared: prepared_gate.is_some(),
        });
        self.fail(FailurePoint::Switch)?;
        let execution = self
            .switch_execution
            .clone()
            .unwrap_or_else(|| SwitchExecution::Committed(target.clone()));
        if let SwitchExecution::Committed(view) = &execution {
            self.set_active(view);
        }
        Ok(execution)
    }

    fn reconcile_two_phase(&mut self, key: &PackageKey) -> Result<ReconcileOutcome, ServiceError> {
        assert_key(key);
        self.record(Call::Reconcile);
        self.fail(FailurePoint::Reconcile)?;
        Ok(self.reconcile_outcome.clone())
    }

    fn rescue_to_base(&mut self, key: &PackageKey) -> Result<RescueExecution, ServiceError> {
        assert_key(key);
        self.record(Call::Rescue);
        self.fail(FailurePoint::Rescue)?;
        Ok(self
            .rescue_execution
            .unwrap_or(RescueExecution::CompletedBase))
    }
}
