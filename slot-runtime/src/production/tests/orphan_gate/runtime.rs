use crate::domain::{GateSnapshot, ManagedPackage, PackageName, PackageObservation, SlotView};
use crate::reconcile::RecoveryBackend;
use crate::runtime::{PlatformError, RuntimeBackend};

use super::probe::base_inodes;

#[derive(Debug)]
pub(super) struct OrphanGateRuntime {
    snapshot: GateSnapshot,
    managed: Option<ManagedPackage>,
    pub(super) held: bool,
    pub(super) restored: Option<GateSnapshot>,
    pub(super) retired: bool,
    emergency_calls: usize,
    pub(super) confirmations: usize,
    pub(super) base_proofs: usize,
}

impl OrphanGateRuntime {
    pub(super) const fn new(snapshot: GateSnapshot) -> Self {
        Self {
            snapshot,
            managed: None,
            held: false,
            restored: None,
            retired: false,
            emergency_calls: 0,
            confirmations: 0,
            base_proofs: 0,
        }
    }

    pub(super) fn enrolled(snapshot: GateSnapshot, managed: ManagedPackage) -> Self {
        Self {
            managed: Some(managed),
            ..Self::new(snapshot)
        }
    }
}

impl RuntimeBackend for OrphanGateRuntime {
    fn observe_package(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        let managed = self
            .managed
            .as_ref()
            .ok_or_else(|| unexpected("observe_package"))?;
        Ok(PackageObservation::new(
            managed.identity().clone(),
            managed.base_inodes(),
            managed.base_inodes(),
            managed.base_inodes(),
            false,
        ))
    }

    fn capture_gate_snapshot(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        Ok(self.snapshot)
    }

    fn acquire_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.held = true;
        Ok(())
    }

    fn verify_gate_held(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.held
            .then_some(())
            .ok_or_else(|| unexpected("gate_open"))
    }

    fn quiesce_processes(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        Ok(())
    }

    fn apply_slot_view(
        &mut self,
        _package: &ManagedPackage,
        _view: &SlotView,
    ) -> Result<(), PlatformError> {
        Err(unexpected("apply_slot_view"))
    }

    fn verify_slot_view(
        &mut self,
        _package: &ManagedPackage,
        _view: &SlotView,
    ) -> Result<(), PlatformError> {
        Err(unexpected("verify_slot_view"))
    }

    fn restore_gate(
        &mut self,
        _package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        if !self.held || snapshot != self.snapshot {
            return Err(unexpected("restore_gate_mismatch"));
        }
        self.restored = Some(snapshot);
        self.held = false;
        Ok(())
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.held || self.restored != Some(self.snapshot) {
            return Err(unexpected("retire_before_exact_restore"));
        }
        self.retired = true;
        Ok(())
    }
}

impl RecoveryBackend for OrphanGateRuntime {
    fn leased_packages(&mut self) -> Result<Vec<PackageName>, PlatformError> {
        Ok(Vec::new())
    }

    fn emergency_gate_if_leased(
        &mut self,
        _package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        self.emergency_calls += 1;
        self.held = true;
        Ok(Some(self.snapshot))
    }

    fn emergency_gate(&mut self, _package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        self.emergency_calls += 1;
        self.held = true;
        Ok(self.snapshot)
    }

    fn confirm_emergency_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if !self.held {
            return Err(unexpected("confirm_gate_open"));
        }
        self.confirmations += 1;
        Ok(())
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        Ok(true)
    }

    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        if package.base_inodes() != base_inodes() {
            return Err(unexpected("native_base_mismatch"));
        }
        self.base_proofs += 1;
        Ok(())
    }
}

fn unexpected(detail: &str) -> PlatformError {
    PlatformError::VerifySlotView {
        detail: detail.to_owned(),
    }
}
