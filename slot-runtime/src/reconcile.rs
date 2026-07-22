#![doc = "Fail-closed reboot and user-unlock reconciliation."]

mod assessment;
mod backend;
mod coordinator;
mod error;
mod finalize;
mod gate;
mod model;
mod restore;
mod restore_committed;
mod transaction;

pub use backend::{NativeBaseRecoveryBackend, RecoveryBackend};
pub use coordinator::Reconciler;
pub use error::ReconcileError;
pub use model::{
    PackageReconcileResult, ReconcileOutcome, ReconcileReason, ReconcileReport, ReconcileScope,
};
