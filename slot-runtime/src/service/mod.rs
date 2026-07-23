#![doc = "Daemon command façade for the fixed user-zero Slots Preview workflow."]

mod commands;
mod error;
mod handler;
mod management_commands;
mod model;
mod outcome;
mod platform;
mod switch_commands;
mod validation;

pub use error::{EnrollmentPublicationError, ServiceError};
pub use model::{
    CapabilitySnapshot, ManagedAppInfo, ObservedGateState, PackageInspection, PackageSnapshot,
    PackageState, RescueExecution, SlotInfo, SwitchExecution,
};
pub use platform::ServicePlatform;

/// Maximum number of validated package targets returned by recovery-only discovery.
pub const MAX_RECOVERY_TARGETS: usize = 64;

use crate::daemon::MutationGuard;

/// Injected, host-testable handler for all six Preview protocol commands.
#[derive(Debug)]
pub struct PreviewService<P> {
    platform: P,
    mutations: MutationGuard,
    mode: ServiceMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceMode {
    Production,
    RecoveryOnly,
}

impl<P> PreviewService<P> {
    /// Creates a service with an independent non-blocking mutation gate.
    pub fn new(platform: P) -> Self {
        Self::with_mutation_guard(platform, MutationGuard::new())
    }

    /// Creates the restricted daemon façade used only for native-base rescue.
    pub fn new_recovery_only(platform: P) -> Self {
        Self {
            platform,
            mutations: MutationGuard::new(),
            mode: ServiceMode::RecoveryOnly,
        }
    }

    /// Creates a service sharing a caller-provided daemon mutation gate.
    pub const fn with_mutation_guard(platform: P, mutations: MutationGuard) -> Self {
        Self {
            platform,
            mutations,
            mode: ServiceMode::Production,
        }
    }

    /// Returns the injected platform for read-only evidence inspection.
    pub const fn platform(&self) -> &P {
        &self.platform
    }

    /// Returns the shared non-blocking mutation gate.
    pub const fn mutation_guard(&self) -> &MutationGuard {
        &self.mutations
    }

    pub(super) const fn recovery_only(&self) -> bool {
        matches!(self.mode, ServiceMode::RecoveryOnly)
    }

    /// Consumes the façade and returns its injected platform.
    pub fn into_platform(self) -> P {
        self.platform
    }
}
