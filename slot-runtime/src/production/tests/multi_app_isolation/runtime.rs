use std::collections::BTreeSet;

use crate::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageName, PackageObservation, SlotView,
};
use crate::launch::{AppLaunchBackend, LaunchDisposition};
use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use crate::runtime::{PlatformError, RuntimeBackend};

#[derive(Debug)]
pub(super) struct MultiPackageRuntime {
    held: BTreeSet<PackageName>,
}

impl MultiPackageRuntime {
    pub(super) const fn new() -> Self {
        Self {
            held: BTreeSet::new(),
        }
    }

    pub(super) fn held(&self, package: &PackageName) -> bool {
        self.held.contains(package)
    }
}

impl RuntimeBackend for MultiPackageRuntime {
    fn observe_package(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        Ok(PackageObservation::new(
            package.identity().clone(),
            package.base_inodes(),
            package.active_inodes(),
            package.active_inodes(),
            false,
        ))
    }

    fn capture_gate_snapshot(&mut self, _: &ManagedPackage) -> Result<GateSnapshot, PlatformError> {
        Ok(gate())
    }

    fn acquire_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.held.insert(package.package_name().clone());
        Ok(())
    }

    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.held(package.package_name())
            .then_some(())
            .ok_or_else(failure)
    }

    fn quiesce_processes(&mut self, _: &ManagedPackage) -> Result<(), PlatformError> {
        Ok(())
    }

    fn apply_slot_view(&mut self, _: &ManagedPackage, _: &SlotView) -> Result<(), PlatformError> {
        Err(failure())
    }

    fn verify_slot_view(&mut self, _: &ManagedPackage, _: &SlotView) -> Result<(), PlatformError> {
        Err(failure())
    }

    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        if snapshot != gate() || !self.held.remove(package.package_name()) {
            return Err(failure());
        }
        Ok(())
    }

    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (!self.held(package.package_name()))
            .then_some(())
            .ok_or_else(failure)
    }
}

impl RecoveryBackend for MultiPackageRuntime {
    fn leased_packages(&mut self) -> Result<Vec<PackageName>, PlatformError> {
        Ok(self.held.iter().cloned().collect())
    }

    fn emergency_gate_if_leased(
        &mut self,
        package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        Ok(self.held(package).then_some(gate()))
    }

    fn emergency_gate(&mut self, package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        self.held.insert(package.clone());
        Ok(gate())
    }

    fn confirm_emergency_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.verify_gate_held(package)
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        Ok(true)
    }

    fn verify_native_base(&mut self, _: &ManagedPackage) -> Result<(), PlatformError> {
        Ok(())
    }
}

impl NativeBaseRecoveryBackend for MultiPackageRuntime {
    fn restore_native_base_unconditionally(
        &mut self,
        _: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        Ok(())
    }

    fn verify_exact_gate_state(
        &mut self,
        _: &ManagedPackage,
        expected: GateSnapshot,
    ) -> Result<bool, PlatformError> {
        Ok(expected == gate())
    }
}

impl AppLaunchBackend for MultiPackageRuntime {
    fn launch_package(&mut self, _: &ManagedPackage) -> LaunchDisposition {
        LaunchDisposition::Failed
    }
}

const fn gate() -> GateSnapshot {
    GateSnapshot::new(PackageEnabledState::Default, false)
}

fn failure() -> PlatformError {
    PlatformError::VerifySlotView {
        detail: "unexpected test backend call".to_owned(),
    }
}
