use crate::domain::{PackageKey, PackageName};

use super::composition::ProductionPlatform;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T> {
    pub(super) fn recovery_overridden(&self, key: &PackageKey) -> bool {
        self.recovery_overrides.contains(key.package_name())
    }

    pub(super) fn recovery_overridden_name(&self, package: &PackageName) -> bool {
        self.recovery_overrides.contains(package)
    }

    pub(super) fn add_recovery_override(&mut self, key: &PackageKey) {
        self.recovery_overrides.insert(key.package_name().clone());
    }

    pub(super) fn clear_recovery_override(&mut self, key: &PackageKey) {
        self.recovery_overrides.remove(key.package_name());
    }
}
