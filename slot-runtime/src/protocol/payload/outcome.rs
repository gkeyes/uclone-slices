use serde::{Deserialize, Serialize};

use crate::domain::SlotId;
use crate::reconcile::ReconcileReason;

/// Reconciliation outcomes exposed by the control plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReconcileOutcome {
    /// The package remains gated during early boot.
    Held,
    /// Credential-encrypted data remains unavailable.
    Locked,
    /// The immutable base view was restored and proved.
    RestoredBase,
    /// A committed logical slot was restored and proved.
    RestoredSlot {
        /// Verified logical slot.
        slot: SlotId,
    },
    /// An incomplete transaction was rolled back.
    RolledBack,
    /// An incomplete transaction was completed to its target.
    RolledForward,
    /// The package remains gated because recovery is not provable.
    RecoveryRequired {
        /// Stable stage at which recovery proof failed.
        reason: ReconcileReason,
    },
    /// The package identity no longer owns the enrolled slots.
    Quarantined,
}
