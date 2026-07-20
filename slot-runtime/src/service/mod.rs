#![doc = "Daemon command façade for the fixed user-zero Slots Preview workflow."]

mod commands;
mod error;
mod handler;
mod management_commands;
mod model;
mod outcome;
mod platform;
mod validation;

pub use error::{EnrollmentPublicationError, ServiceError};
pub use model::{
    CapabilitySnapshot, ManagedAppInfo, ObservedGateState, PackageInspection, PackageSnapshot,
    PackageState, RescueExecution, SlotInfo, SwitchExecution,
};
pub use platform::ServicePlatform;

use crate::daemon::MutationGuard;

/// Injected, host-testable handler for all six Preview protocol commands.
#[derive(Debug)]
pub struct PreviewService<P> {
    platform: P,
    mutations: MutationGuard,
}

impl<P> PreviewService<P> {
    /// Creates a service with an independent non-blocking mutation gate.
    pub fn new(platform: P) -> Self {
        Self::with_mutation_guard(platform, MutationGuard::new())
    }

    /// Creates a service sharing a caller-provided daemon mutation gate.
    pub const fn with_mutation_guard(platform: P, mutations: MutationGuard) -> Self {
        Self {
            platform,
            mutations,
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

    /// Consumes the façade and returns its injected platform.
    pub fn into_platform(self) -> P {
        self.platform
    }
}
