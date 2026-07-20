use crate::domain::{GateSnapshot, ManagedPackage, PackageName};
use crate::runtime::{PlatformError, RuntimeBackend};

#[doc = "Platform operations needed only while reconciling a rebooted user0 view."]
pub trait RecoveryBackend: RuntimeBackend {
    #[doc = "Enumerates recognizable active gate leases before enrollment metadata is trusted."]
    fn leased_packages(&mut self) -> Result<Vec<PackageName>, PlatformError>;

    #[doc = "Gates the allowlisted package only when a durable orphan lease artifact exists."]
    fn emergency_gate_if_leased(
        &mut self,
        package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError>;

    #[doc = "Captures the exact gate state, persists an unverified lease, and holds the package."]
    fn emergency_gate(&mut self, package: &PackageName) -> Result<GateSnapshot, PlatformError>;

    #[doc = "Upgrades an emergency lease with validated user0 enrollment anchors."]
    fn confirm_emergency_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;

    #[doc = "Returns whether user0 credential-encrypted storage is unlocked."]
    fn user0_unlocked(&mut self) -> Result<bool, PlatformError>;

    #[doc = "Proves native CE and DE base inodes with no runtime bind mounted."]
    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError>;
}

#[doc = "Recovery-only boundary that retires validated Preview mounts to native base."]
pub trait NativeBaseRecoveryBackend: core::fmt::Debug {
    #[doc = "Removes only validated Preview bind layers and proves immutable native base."]
    fn restore_native_base_unconditionally(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError>;

    #[doc = "Checks the current enabled and suspended state without mutating it."]
    fn verify_exact_gate_state(
        &mut self,
        package: &ManagedPackage,
        expected: GateSnapshot,
    ) -> Result<bool, PlatformError>;
}

impl<T> RecoveryBackend for &mut T
where
    T: RecoveryBackend + ?Sized,
{
    fn leased_packages(&mut self) -> Result<Vec<PackageName>, PlatformError> {
        (**self).leased_packages()
    }

    fn emergency_gate_if_leased(
        &mut self,
        package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        (**self).emergency_gate_if_leased(package)
    }

    fn emergency_gate(&mut self, package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        (**self).emergency_gate(package)
    }

    fn confirm_emergency_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).confirm_emergency_gate(package)
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        (**self).user0_unlocked()
    }

    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        (**self).verify_native_base(package)
    }
}

impl<T> NativeBaseRecoveryBackend for &mut T
where
    T: NativeBaseRecoveryBackend + ?Sized,
{
    fn restore_native_base_unconditionally(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        (**self).restore_native_base_unconditionally(package)
    }

    fn verify_exact_gate_state(
        &mut self,
        package: &ManagedPackage,
        expected: GateSnapshot,
    ) -> Result<bool, PlatformError> {
        (**self).verify_exact_gate_state(package, expected)
    }
}
