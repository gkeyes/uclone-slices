use uclone_slot_runtime::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageName, PackageObservation, SlotId,
    SlotView,
};
use uclone_slot_runtime::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent rescue fault toggles"
)]
pub(crate) struct FakeRescueBackend {
    observation: PackageObservation,
    current_view: SlotView,
    gate_original: GateSnapshot,
    gate_current: GateSnapshot,
    gate_held: bool,
    lease_present: bool,
    retired_evidence: bool,
    reject_emergency_gate: bool,
    reject_retire_gate_lease: bool,
    pub(crate) emergency_gate_calls: usize,
    pub(crate) native_restore_calls: usize,
    pub(crate) gate_restore_calls: usize,
    pub(crate) retire_calls: usize,
}
impl FakeRescueBackend {
    pub(crate) fn preview(package: &ManagedPackage) -> Self {
        let base = package.base_inodes();
        let preview = uclone_slot_runtime::domain::DataInodes::new(301, 401).unwrap();
        let snapshot = GateSnapshot::new(PackageEnabledState::Enabled, true);
        Self {
            observation: PackageObservation::new(
                package.identity().clone(),
                base,
                preview,
                preview,
                false,
            ),
            current_view: SlotView::new(SlotId::parse("preview").unwrap(), preview),
            gate_original: snapshot,
            gate_current: snapshot,
            gate_held: false,
            lease_present: false,
            retired_evidence: false,
            reject_emergency_gate: false,
            reject_retire_gate_lease: false,
            emergency_gate_calls: 0,
            native_restore_calls: 0,
            gate_restore_calls: 0,
            retire_calls: 0,
        }
    }
    pub(crate) fn set_observation(&mut self, observation: PackageObservation) {
        self.observation = observation;
    }
    pub(crate) fn drift_gate(&mut self, snapshot: GateSnapshot) {
        self.gate_current = snapshot;
        self.gate_held = snapshot.enabled_state() == PackageEnabledState::DisabledUser;
    }
    pub(crate) const fn reject_emergency_gate(&mut self) {
        self.reject_emergency_gate = true;
    }
    pub(crate) const fn reject_retire_gate_lease(&mut self) {
        self.reject_retire_gate_lease = true;
    }
    pub(crate) const fn gate_current(&self) -> GateSnapshot {
        self.gate_current
    }
    pub(crate) const fn gate_held(&self) -> bool {
        self.gate_held
    }
    pub(crate) const fn lease_present(&self) -> bool {
        self.lease_present
    }

    pub(crate) fn native_base(&self, package: &ManagedPackage) -> bool {
        self.current_view == SlotView::new(SlotId::base(), package.base_inodes())
    }

    const fn hold_gate(&mut self) {
        self.gate_current = GateSnapshot::new(
            PackageEnabledState::DisabledUser,
            self.gate_current.suspended(),
        );
        self.gate_held = true;
    }
}

impl RuntimeBackend for FakeRescueBackend {
    fn observe_package(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        Ok(self.observation.clone())
    }

    fn capture_gate_snapshot(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        if !self.lease_present {
            self.gate_original = self.gate_current;
            self.lease_present = true;
        }
        Ok(self.gate_original)
    }

    fn acquire_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.hold_gate();
        Ok(())
    }

    fn verify_gate_held(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.gate_held {
            Ok(())
        } else {
            Err(PlatformError::VerifyGateHeld {
                detail: "fake_gate_open".to_owned(),
            })
        }
    }

    fn quiesce_processes(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.verify_gate_held(package)
    }

    fn apply_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.current_view = view.clone();
        Ok(())
    }

    fn verify_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        if &self.current_view == view {
            Ok(())
        } else {
            Err(PlatformError::view_verification("fake_view_mismatch"))
        }
    }

    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        self.verify_gate_held(package)?;
        if !self.lease_present || snapshot != self.gate_original {
            return Err(PlatformError::RestoreGate {
                detail: "fake_gate_lease_mismatch".to_owned(),
            });
        }
        self.gate_restore_calls += 1;
        self.gate_current = snapshot;
        self.gate_held = false;
        Ok(())
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.retire_calls += 1;
        if self.reject_retire_gate_lease {
            return Err(PlatformError::RetireGateLease {
                detail: "injected_retire_gate_failure".to_owned(),
            });
        }
        if self.retired_evidence && !self.lease_present {
            return Ok(());
        }
        if !self.lease_present || self.gate_current != self.gate_original {
            return Err(PlatformError::RetireGateLease {
                detail: "fake_gate_not_exact".to_owned(),
            });
        }
        self.retired_evidence = true;
        self.lease_present = false;
        Ok(())
    }
}

impl RecoveryBackend for FakeRescueBackend {
    fn emergency_gate_if_leased(
        &mut self,
        _package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        if !self.lease_present {
            return Ok(None);
        }
        self.emergency_gate_calls += 1;
        self.hold_gate();
        Ok(Some(self.gate_original))
    }

    fn emergency_gate(&mut self, _package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        if self.reject_emergency_gate {
            return Err(PlatformError::AcquireGate {
                detail: "injected_emergency_gate_failure".to_owned(),
            });
        }
        self.emergency_gate_calls += 1;
        if !self.lease_present {
            self.gate_original = self.gate_current;
            self.lease_present = true;
        }
        self.hold_gate();
        Ok(self.gate_original)
    }

    fn confirm_emergency_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.verify_gate_held(package)
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        Ok(true)
    }

    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.native_base(package) {
            Ok(())
        } else {
            Err(PlatformError::view_verification("fake_not_native_base"))
        }
    }
}

impl NativeBaseRecoveryBackend for FakeRescueBackend {
    fn restore_native_base_unconditionally(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        self.verify_gate_held(package)?;
        self.native_restore_calls += 1;
        self.current_view = SlotView::new(SlotId::base(), package.base_inodes());
        Ok(())
    }

    fn verify_exact_gate_state(
        &mut self,
        _package: &ManagedPackage,
        expected: GateSnapshot,
    ) -> Result<bool, PlatformError> {
        Ok(self.gate_current == expected)
    }
}
