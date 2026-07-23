#![doc = "Daemon command façade for the fixed user-zero Slots Preview workflow."]

use std::sync::Arc;

mod commands;
mod diagnostic;
mod error;
mod handler;
mod management_commands;
mod model;
mod outcome;
mod platform;
mod switch_commands;
mod validation;

pub use diagnostic::{
    DiagnosticCause, DiagnosticCommand, DiagnosticFailure, DiagnosticSink, NoopDiagnosticSink,
    OperationContext, OperationPhase,
};
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
    diagnostics: Option<Arc<dyn DiagnosticSink>>,
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
            diagnostics: None,
        }
    }

    #[doc = "Creates a recovery-only service with an injected diagnostic sink."]
    pub fn with_recovery_only_diagnostic_sink<S>(platform: P, sink: S) -> Self
    where
        S: DiagnosticSink + 'static,
    {
        Self::with_mode_and_diagnostic_sink(
            platform,
            MutationGuard::new(),
            ServiceMode::RecoveryOnly,
            Some(Arc::new(sink)),
        )
    }

    /// Creates a service sharing a caller-provided daemon mutation gate.
    pub const fn with_mutation_guard(platform: P, mutations: MutationGuard) -> Self {
        Self {
            platform,
            mutations,
            mode: ServiceMode::Production,
            diagnostics: None,
        }
    }

    #[doc = "Creates a production service with an injected diagnostic sink."]
    pub fn with_diagnostic_sink<S>(platform: P, sink: S) -> Self
    where
        S: DiagnosticSink + 'static,
    {
        Self::with_mutation_guard_and_diagnostic_sink(platform, MutationGuard::new(), sink)
    }

    #[doc = "Creates a production service with shared mutation and diagnostic seams."]
    pub fn with_mutation_guard_and_diagnostic_sink<S>(
        platform: P,
        mutations: MutationGuard,
        sink: S,
    ) -> Self
    where
        S: DiagnosticSink + 'static,
    {
        Self::with_mode_and_diagnostic_sink(
            platform,
            mutations,
            ServiceMode::Production,
            Some(Arc::new(sink)),
        )
    }

    fn with_mode_and_diagnostic_sink(
        platform: P,
        mutations: MutationGuard,
        mode: ServiceMode,
        diagnostics: Option<Arc<dyn DiagnosticSink>>,
    ) -> Self {
        Self {
            platform,
            mutations,
            mode,
            diagnostics,
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
