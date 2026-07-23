use crate::lifecycle::LifecycleState;

use super::SlotId;

mod resolver;

#[cfg(test)]
mod tests;

/// Amount of platform evidence available while resolving one package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceScope {
    /// User zero is unlocked and CE, DE, package, view, and gate facts may be observed.
    Unlocked,
    /// Early boot may inspect durable DE-safe evidence but must not depend on CE.
    EarlyBootLocked,
    /// The daemon exposes only bounded reconciliation and rescue operations.
    RecoveryOnly,
}

/// Durable facts loaded before consulting Android live state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurablePackageFacts {
    /// No enrollment or other package management evidence exists.
    Absent,
    /// An enrollment attempt still fences package publication.
    EnrollmentAttempt,
    /// Management evidence exists without a complete enrollment.
    Orphaned,
    /// A durable invariant could not be proved.
    Incomplete(InvariantViolation),
    /// Enrollment, catalog, Journal, Registry, metadata, and lifecycle evidence agree.
    Enrolled {
        /// Persisted package lifecycle.
        lifecycle: LifecycleState,
        /// Durable active-slot conclusion.
        active_slot: SlotId,
    },
}

/// Coherent live-package conclusion derived from one Android observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePackageFacts {
    /// Live evidence was not required for the durable conclusion.
    NotObserved,
    /// Identity, package anchors, and visible view agree with the durable contract.
    Healthy,
    /// Live evidence requires fail-closed reconciliation.
    RecoveryRequired(InvariantViolation),
    /// Installed identity or package class must remain isolated.
    Quarantined(InvariantViolation),
}

/// Exact Android execution-gate observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFacts {
    /// Gate evidence was not required for the preceding conclusion.
    NotObserved,
    /// Enabled and suspended state were read successfully.
    Observed,
    /// Exact gate state could not be read.
    Unavailable,
}

/// Pure facts consumed by the package aggregate resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFacts {
    scope: EvidenceScope,
    durable: DurablePackageFacts,
    live: LivePackageFacts,
    gate: GateFacts,
}

impl PackageFacts {
    /// Groups already-validated durable, live, and gate facts.
    pub const fn new(
        scope: EvidenceScope,
        durable: DurablePackageFacts,
        live: LivePackageFacts,
        gate: GateFacts,
    ) -> Self {
        Self {
            scope,
            durable,
            live,
            gate,
        }
    }
}

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
    /// No managed package exists, so no Slots gate is required.
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
    /// A package transaction has not reached Completed.
    UnfinishedTransaction,
    /// Catalog entries do not prove the enrolled Base and extension views.
    CatalogMismatch,
    /// Registry evidence cannot prove the active catalog view.
    ActiveViewUnproven,
    /// Active extension metadata is missing or not Ready.
    ActiveSlotMetadataUnready,
    /// Registry commit evidence does not match a completed Journal transaction.
    RegistryJournalMismatch,
    /// A persisted non-normal lifecycle has no accepted use-case proof.
    PersistedLifecycleState,
    /// The enrolled package contract could not be reconstructed.
    ManagedPackageInvalid,
    /// Android live package evidence could not be read.
    LiveObservationUnavailable,
    /// The package support class is blocked.
    PackageSupportBlocked,
    /// Compatibility acceptance does not cover the observed identity.
    CompatibilityPolicyMismatch,
    /// UID or signing identity no longer owns the enrolled slots.
    IdentityChanged,
    /// PackageManager Base anchors changed.
    PackageManagerInodeDrift,
    /// Canonical or App-process view differs from the committed slot.
    VisibleViewDrift,
    /// APK replacement occurred outside a proved update window.
    UnexpectedPackageReplacement,
    /// A managed update requires an explicit safe window.
    UpdateWindowRequired,
    /// Exact enabled and suspended state could not be read.
    GateObservationUnavailable,
}

/// Pure package conclusion used to shadow the current production loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageAggregate {
    state: AggregateState,
    active_slot: Option<SlotId>,
    safety: SafetyDisposition,
    allowed_actions: Vec<AllowedAction>,
    violation: Option<InvariantViolation>,
}

impl PackageAggregate {
    /// Resolves one package without filesystem, Android, Shell, or protocol access.
    pub fn resolve(facts: PackageFacts) -> Self {
        resolver::resolve(facts)
    }

    /// Returns the package state conclusion.
    pub const fn state(&self) -> AggregateState {
        self.state
    }

    /// Returns the proved active slot when Ready.
    pub const fn active_slot(&self) -> Option<&SlotId> {
        self.active_slot.as_ref()
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
