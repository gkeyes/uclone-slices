use super::super::ServiceError;
use super::context::OperationContext;

#[doc = "Internal causes kept outside the protocol response."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCause {
    #[doc = "The typed request violated the service boundary."]
    InvalidInput,
    #[doc = "A device or package capability policy rejected the operation."]
    CapabilityPolicy,
    #[doc = "A durable read could not establish package state."]
    StateRead,
    #[doc = "A durable state or proof invariant was not satisfied."]
    StateInvariant,
    #[doc = "A bounded persistence operation failed."]
    Storage,
    #[doc = "The installed package identity drifted from enrollment."]
    IdentityDrift,
    #[doc = "A gate snapshot or restoration proof did not match."]
    GateMismatch,
    #[doc = "The live mount view did not match the requested proof."]
    MountMismatch,
    #[doc = "A request failed before service decoding."]
    ProtocolDecode,
    #[doc = "A success payload failed the response boundary."]
    ProtocolEncode,
    #[doc = "The mutation gate or expected state conflicted."]
    LockConflict,
    #[doc = "Containment was required but not proved complete."]
    ContainmentFailure,
    #[doc = "The bounded operation exceeded its time budget."]
    Timeout,
    #[doc = "No narrower internal cause is currently proved."]
    Unknown,
}

impl DiagnosticCause {
    #[doc = "Maps an observable service error to its currently provable cause."]
    pub const fn from_service_error(error: ServiceError) -> Self {
        match error {
            ServiceError::InvalidRequest => Self::InvalidInput,
            ServiceError::PackageNotAllowed
            | ServiceError::DirectBootConfirmationRequired
            | ServiceError::UnsupportedDevice => Self::CapabilityPolicy,
            ServiceError::Busy | ServiceError::Conflict => Self::LockConflict,
            ServiceError::NotFound
            | ServiceError::RecoveryRequired
            | ServiceError::Quarantined
            | ServiceError::UserLocked
            | ServiceError::Internal => Self::Unknown,
        }
    }
}

impl From<ServiceError> for DiagnosticCause {
    fn from(error: ServiceError) -> Self {
        Self::from_service_error(error)
    }
}

#[doc = "Internal phase reached while handling one request."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationPhase {
    #[doc = "A safe request context was constructed."]
    RequestReceived,
    #[doc = "Recovery-only mode rejected the command."]
    RecoveryGate,
    #[doc = "The mutation gate was being reserved."]
    MutationGate,
    #[doc = "Mutation capability policy was checked."]
    CapabilityCheck,
    #[doc = "The typed service operation was dispatched."]
    Dispatch,
    #[doc = "The success payload failed response validation."]
    ProtocolEncode,
}

#[doc = "Typed command name retained in internal context."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCommand {
    #[doc = "Runtime capability probe."]
    Probe,
    #[doc = "Package compatibility inspection."]
    InspectPackage,
    #[doc = "Managed package listing."]
    ListManagedApps,
    #[doc = "Recovery target listing."]
    ListRecoveryTargets,
    #[doc = "Package status read."]
    StatusPackage,
    #[doc = "Coherent package and slot snapshot read."]
    PackageSnapshot,
    #[doc = "Package enrollment."]
    EnrollPackage,
    #[doc = "Slot creation."]
    CreateSlot,
    #[doc = "Slot listing."]
    ListSlots,
    #[doc = "Active view switch."]
    Switch,
    #[doc = "Current-view launch."]
    LaunchCurrent,
    #[doc = "Display-only slot rename."]
    RenameSlot,
    #[doc = "Slot deletion."]
    DeleteSlot,
    #[doc = "All-package reconciliation."]
    Reconcile,
    #[doc = "Single-package reconciliation."]
    ReconcilePackage,
    #[doc = "Package retirement."]
    RetirePackage,
    #[doc = "Native-base rescue."]
    RescueToBase,
}

#[doc = "Structured failure delivered to a diagnostic sink."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFailure {
    context: OperationContext,
    cause: DiagnosticCause,
    service_error: ServiceError,
}

impl DiagnosticFailure {
    #[doc = "Creates a failure with explicit cause and service classification."]
    pub const fn new(
        context: OperationContext,
        cause: DiagnosticCause,
        service_error: ServiceError,
    ) -> Self {
        Self {
            context,
            cause,
            service_error,
        }
    }

    #[doc = "Creates a failure using the currently provable cause mapping."]
    pub const fn from_service_error(context: OperationContext, error: ServiceError) -> Self {
        Self::new(context, DiagnosticCause::from_service_error(error), error)
    }

    #[doc = "Returns the safe operation context."]
    pub const fn context(&self) -> &OperationContext {
        &self.context
    }
    #[doc = "Returns the internal cause category."]
    pub const fn cause(&self) -> DiagnosticCause {
        self.cause
    }
    #[doc = "Returns the non-wire service classification."]
    pub const fn service_error(&self) -> ServiceError {
        self.service_error
    }
}
