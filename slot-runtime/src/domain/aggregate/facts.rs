use crate::lifecycle::LifecycleState;

use super::InvariantViolation;
use crate::domain::{GateSnapshot, ManagedPackage, SlotId, SlotView};

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

/// Complete evidence retained only by a Ready package conclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyPackageEvidence {
    managed: ManagedPackage,
    slots: Vec<SlotView>,
    gate: GateSnapshot,
}

impl ReadyPackageEvidence {
    /// Groups the exact package, catalog, and gate facts used by the Ready decision.
    pub const fn new(managed: ManagedPackage, slots: Vec<SlotView>, gate: GateSnapshot) -> Self {
        Self {
            managed,
            slots,
            gate,
        }
    }

    /// Returns the validated managed package.
    pub const fn managed(&self) -> &ManagedPackage {
        &self.managed
    }

    /// Returns every validated non-base slot.
    pub fn slots(&self) -> &[SlotView] {
        &self.slots
    }

    /// Returns the exact Android gate observation.
    pub const fn gate(&self) -> GateSnapshot {
        self.gate
    }

    /// Separates the evidence for the legacy package snapshot adapter.
    pub fn into_parts(self) -> (ManagedPackage, Vec<SlotView>, GateSnapshot) {
        (self.managed, self.slots, self.gate)
    }
}

/// Pure facts consumed by the package aggregate resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFacts {
    pub(super) scope: EvidenceScope,
    pub(super) durable: DurablePackageFacts,
    pub(super) live: LivePackageFacts,
    pub(super) gate: GateFacts,
    pub(super) ready_evidence: Option<ReadyPackageEvidence>,
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
            ready_evidence: None,
        }
    }

    /// Groups a healthy fact set with the exact evidence required by a Ready result.
    pub const fn with_ready_evidence(
        scope: EvidenceScope,
        durable: DurablePackageFacts,
        live: LivePackageFacts,
        gate: GateFacts,
        ready_evidence: ReadyPackageEvidence,
    ) -> Self {
        Self {
            scope,
            durable,
            live,
            gate,
            ready_evidence: Some(ready_evidence),
        }
    }
}
