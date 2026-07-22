use crate::domain::{GateSnapshot, ManagedPackage, PackageName, SlotId, TransactionId};
use crate::registry::PackageRevision;
use serde::{Deserialize, Serialize};

#[doc = "Limits reconciliation side effects to one package or the complete boot set."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileScope {
    #[doc = "Reconciles every discovered managed package during daemon startup."]
    All,
    #[doc = "Reconciles only the exact package requested by the control plane."]
    Package(PackageName),
}

#[doc = "Stable reason why a package remains execution-gated."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileReason {
    #[doc = "A durable gate lease has no validated enrollment anchors."]
    EnrollmentMetadata,
    #[doc = "Journal enumeration or integrity could not be proved."]
    JournalMetadata,
    #[doc = "Registry integrity or package ownership could not be proved."]
    RegistryMetadata,
    #[doc = "More than one unfinished transaction targets the package."]
    AmbiguousTransactions,
    #[doc = "The package execution gate could not be proved held."]
    GateHoldFailed,
    #[doc = "No exact pre-gate enabled and suspended snapshot is available."]
    MissingGateSnapshot,
    #[doc = "Installed package metadata drifted from enrollment."]
    PackageStateDrift,
    #[doc = "A conditional Direct Boot package cannot release a non-base reboot view yet."]
    ConditionalDirectBoot,
    #[doc = "The required CE and DE view could not be restored and proved."]
    ViewRestoreFailed,
    #[doc = "The exact package gate state could not be restored."]
    GateRestoreFailed,
    #[doc = "The durable gate lease could not be retired after recovery completed."]
    GateLeaseRetirement,
    #[doc = "A recovery journal transition could not be made durable."]
    JournalUpdateFailed,
    #[doc = "Journal and Registry do not prove one recovery side."]
    CommitPointUncertain,
}

impl ReconcileReason {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::EnrollmentMetadata => "enrollment_metadata",
            Self::JournalMetadata => "journal_metadata",
            Self::RegistryMetadata => "registry_metadata",
            Self::AmbiguousTransactions => "ambiguous_transactions",
            Self::GateHoldFailed => "gate_hold_failed",
            Self::MissingGateSnapshot => "missing_gate_snapshot",
            Self::PackageStateDrift => "package_state_drift",
            Self::ConditionalDirectBoot => "conditional_direct_boot",
            Self::ViewRestoreFailed => "view_restore_failed",
            Self::GateRestoreFailed => "gate_restore_failed",
            Self::GateLeaseRetirement => "gate_lease_retirement",
            Self::JournalUpdateFailed => "journal_update_failed",
            Self::CommitPointUncertain => "commit_point_uncertain",
        }
    }
}

#[doc = "Observable reconciliation result for one enrolled package."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    #[doc = "Early boot proved the package gate held without reading CE data."]
    Held,
    #[doc = "User0 remains locked, so no CE operation was attempted."]
    Locked,
    #[doc = "The native unbound base view was proved before gate restoration."]
    RestoredBase,
    #[doc = "A committed non-base CE and DE pair was applied and proved."]
    RestoredSlot(SlotId),
    #[doc = "An incomplete transaction was rolled back to its proved previous view."]
    RolledBack,
    #[doc = "An incomplete transaction was completed to its proved target view."]
    RolledForward,
    #[doc = "Uncertainty retained the package execution gate."]
    RecoveryRequired(ReconcileReason),
    #[doc = "The installed UID or signing identity no longer owns the slots."]
    Quarantined,
}

impl ReconcileOutcome {
    pub(super) const fn releases_gate(&self) -> bool {
        matches!(
            self,
            Self::RestoredBase | Self::RestoredSlot(_) | Self::RolledBack | Self::RolledForward
        )
    }
}

#[doc = "One enrolled or orphan-leased package reconciliation result."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReconcileResult {
    package_name: PackageName,
    transaction_id: Option<TransactionId>,
    outcome: ReconcileOutcome,
}

impl PackageReconcileResult {
    pub(super) const fn new(
        package_name: PackageName,
        transaction_id: Option<TransactionId>,
        outcome: ReconcileOutcome,
    ) -> Self {
        Self {
            package_name,
            transaction_id,
            outcome,
        }
    }

    #[doc = "Returns the enrolled or orphan-leased package name."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the unfinished transaction involved, when any."]
    pub const fn transaction_id(&self) -> Option<&TransactionId> {
        self.transaction_id.as_ref()
    }

    #[doc = "Returns the package reconciliation outcome."]
    pub const fn outcome(&self) -> &ReconcileOutcome {
        &self.outcome
    }
}

#[doc = "Results for one early-boot or unlocked reconciliation pass."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    results: Vec<PackageReconcileResult>,
}

impl ReconcileReport {
    pub(super) const fn new(results: Vec<PackageReconcileResult>) -> Self {
        Self { results }
    }

    #[doc = "Returns package results in package-name order."]
    pub fn results(&self) -> &[PackageReconcileResult] {
        &self.results
    }
}

#[derive(Debug, Clone)]
pub(super) enum JournalMetadata {
    Clean,
    Transaction(TransactionId),
    Ambiguous,
    Invalid,
}

#[derive(Debug, Clone)]
pub(super) enum RegistryMetadata {
    Valid(Box<Option<PackageRevision>>),
    Invalid,
}

#[derive(Debug, Clone)]
pub(super) struct HeldPackage {
    pub(super) managed: ManagedPackage,
    pub(super) snapshot: Option<GateSnapshot>,
    pub(super) journal: JournalMetadata,
    pub(super) registry: RegistryMetadata,
    pub(super) gate_proved: bool,
}

impl HeldPackage {
    pub(super) const fn transaction_id(&self) -> Option<&TransactionId> {
        match &self.journal {
            JournalMetadata::Transaction(value) => Some(value),
            JournalMetadata::Clean | JournalMetadata::Ambiguous | JournalMetadata::Invalid => None,
        }
    }
}
