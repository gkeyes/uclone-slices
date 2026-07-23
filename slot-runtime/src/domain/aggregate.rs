mod facts;
mod resolver;

#[cfg(test)]
mod tests;

use super::SlotId;

pub use facts::{
    DurablePackageFacts, EvidenceScope, GateFacts, LivePackageFacts, PackageFacts,
    ReadyPackageEvidence,
};

/// Stable package state produced by the pure resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateState {
    /// No package management evidence exists.
    Absent,
    /// Durable and live evidence prove a usable package view.
    Ready,
    /// Evidence is incomplete, ambiguous, or unsafe.
    RecoveryRequired,
    /// The package identity or compatibility class is isolated.
    Quarantined,
}

/// Package action admitted by a resolved aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedAction {
    /// Inspect device and package facts without mutating state.
    Inspect,
    /// Publish a new enrollment.
    Enroll,
    /// Read the current package and slot status.
    ReadStatus,
    /// Create, switch, rename, or delete a slot through a typed use case.
    MutateSlots,
    /// Reconcile durable and live evidence.
    Reconcile,
    /// Perform the bounded native-Base rescue path.
    RescueToBase,
    /// Launch only the freshly verified active package view.
    Launch,
}

/// Required containment posture for one aggregate conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyDisposition {
    /// No managed package exists, so no `Slots` gate is required.
    NoContainmentRequired,
    /// Exact gate facts were observed and normal use may preserve them.
    PreserveObservedGate,
    /// The package must remain gated until reconciliation proves a view.
    HoldGate,
    /// Old slots must remain isolated from the installed package identity.
    Quarantine,
}

/// First invariant that prevents a normal Ready conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvariantViolation {
    /// A store could not be read and verified.
    StoreUnreadable,
    /// An enrollment attempt has not reached an unambiguous terminal state.
    EnrollmentAttemptPending,
    /// Package management evidence exists without enrollment.
    OrphanedManagementEvidence,
    /// The persisted package lifecycle stream is missing.
    MissingPackageState,
    /// Package lifecycle evidence names another package key.
    PackageStateMismatch,
    /// A package transaction has not reached `Completed`.
    UnfinishedTransaction,
    /// `Catalog` entries do not prove the enrolled `Base` and extension views.
    CatalogMismatch,
    /// `Registry` evidence cannot prove the active catalog view.
    ActiveViewUnproven,
    /// Active extension metadata is missing or not `Ready`.
    ActiveSlotMetadataUnready,
    /// Registry commit evidence does not match a completed `Journal` transaction.
    RegistryJournalMismatch,
    /// A persisted non-normal lifecycle has no accepted use-case proof.
    PersistedLifecycleState,
    /// The enrolled package contract could not be reconstructed.
    ManagedPackageInvalid,
    /// `Android` live package evidence could not be read.
    LiveObservationUnavailable,
    /// The package support class is blocked.
    PackageSupportBlocked,
    /// Compatibility acceptance does not cover the observed identity.
    CompatibilityPolicyMismatch,
    /// `UID` or signing identity no longer owns the enrolled slots.
    IdentityChanged,
    /// `PackageManager` Base anchors changed.
    PackageManagerInodeDrift,
    /// Canonical or `App`-process view differs from the committed slot.
    VisibleViewDrift,
    /// `APK` replacement occurred outside a proved update window.
    UnexpectedPackageReplacement,
    /// A managed update requires an explicit safe window.
    UpdateWindowRequired,
    /// Exact enabled and suspended `Gate` state could not be read.
    GateObservationUnavailable,
    /// A `Ready` decision did not retain its exact package, slot, and gate evidence.
    ReadyEvidenceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AggregateKind {
    Absent,
    Ready(ReadyPackageEvidence),
    RecoveryRequired,
    Quarantined,
}

/// Pure package conclusion used by the production package-state loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageAggregate {
    kind: AggregateKind,
    safety: SafetyDisposition,
    allowed_actions: Vec<AllowedAction>,
    violation: Option<InvariantViolation>,
}

impl PackageAggregate {
    /// Resolves one package without filesystem, `Android`, `Shell`, or protocol access.
    pub fn resolve(facts: PackageFacts) -> Self {
        resolver::resolve(facts)
    }

    /// Returns the package state conclusion.
    pub const fn state(&self) -> AggregateState {
        match &self.kind {
            AggregateKind::Absent => AggregateState::Absent,
            AggregateKind::Ready(_) => AggregateState::Ready,
            AggregateKind::RecoveryRequired => AggregateState::RecoveryRequired,
            AggregateKind::Quarantined => AggregateState::Quarantined,
        }
    }

    /// Returns the proved active slot when `Ready`.
    pub const fn active_slot(&self) -> Option<&SlotId> {
        match &self.kind {
            AggregateKind::Ready(evidence) => Some(evidence.managed().active_slot()),
            AggregateKind::Absent
            | AggregateKind::RecoveryRequired
            | AggregateKind::Quarantined => None,
        }
    }

    /// Returns the complete `Ready` evidence, or none for every non-`Ready` conclusion.
    pub const fn ready_evidence(&self) -> Option<&ReadyPackageEvidence> {
        match &self.kind {
            AggregateKind::Ready(evidence) => Some(evidence),
            AggregateKind::Absent
            | AggregateKind::RecoveryRequired
            | AggregateKind::Quarantined => None,
        }
    }

    /// Consumes the aggregate and returns complete `Ready` evidence only for `Ready`.
    pub fn into_ready_evidence(self) -> Option<ReadyPackageEvidence> {
        match self.kind {
            AggregateKind::Ready(evidence) => Some(evidence),
            AggregateKind::Absent
            | AggregateKind::RecoveryRequired
            | AggregateKind::Quarantined => None,
        }
    }

    /// Returns the required containment posture.
    pub const fn safety(&self) -> SafetyDisposition {
        self.safety
    }

    /// Returns the actions admitted by this exact evidence scope.
    pub fn allowed_actions(&self) -> &[AllowedAction] {
        &self.allowed_actions
    }

    /// Returns the first fail-closed invariant violation, if any.
    pub const fn violation(&self) -> Option<InvariantViolation> {
        self.violation
    }
}
