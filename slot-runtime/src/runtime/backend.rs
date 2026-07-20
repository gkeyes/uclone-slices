use crate::domain::{GateSnapshot, ManagedPackage, PackageObservation, SlotView};

use super::PlatformError;

#[doc = "Typed Android platform boundary required by the slot transaction coordinator."]
pub trait RuntimeBackend: core::fmt::Debug {
    #[doc = "Samples package identity and every inode view used by the lifecycle guard."]
    fn observe_package(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError>;

    #[doc = "Captures the exact enabled and suspended state before gate acquisition."]
    fn capture_gate_snapshot(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError>;

    #[doc = "Acquires the package execution gate without changing its captured snapshot."]
    fn acquire_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;

    #[doc = "Proves that the package execution gate is held."]
    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;

    #[doc = "Stops and verifies every process belonging to the package."]
    fn quiesce_processes(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;

    #[doc = "Applies one paired CE and DE view as a single platform operation."]
    fn apply_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError>;

    #[doc = "Verifies the canonical, mirror, Zygote, CE, and DE view."]
    fn verify_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError>;

    #[doc = "Restores the exact enabled and suspended state while retaining its recovery lease."]
    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError>;

    #[doc = "Retires the validated gate lease only after gate release is durably recorded."]
    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;
}

impl<T> RuntimeBackend for &mut T
where
    T: RuntimeBackend + ?Sized,
{
    fn observe_package(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        (**self).observe_package(package)
    }

    fn capture_gate_snapshot(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        (**self).capture_gate_snapshot(package)
    }

    fn acquire_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).acquire_gate(package)
    }

    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).verify_gate_held(package)
    }

    fn quiesce_processes(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).quiesce_processes(package)
    }

    fn apply_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        (**self).apply_slot_view(package, view)
    }

    fn verify_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        (**self).verify_slot_view(package, view)
    }

    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        (**self).restore_gate(package, snapshot)
    }

    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).retire_gate_lease(package)
    }
}
