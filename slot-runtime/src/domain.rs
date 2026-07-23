#![doc = "Validated persistence and command-boundary values."]

mod aggregate;
mod error;
mod gate;
mod identifier;
mod managed_update;
mod package;
mod storage;
mod transaction_token;

pub use aggregate::{
    AggregateState, AllowedAction, DurablePackageFacts, EvidenceScope, GateFacts,
    InvariantViolation, LivePackageFacts, PackageAggregate, PackageFacts, ReadyPackageEvidence,
    SafetyDisposition,
};
pub use error::DomainError;
pub use gate::{GateSnapshot, PackageEnabledState};
pub use identifier::{PackageName, SlotId};
pub use managed_update::{ManagedUpdateContext, UpdateToken};
pub use package::{
    AppIdentity, AppOwnerIdentityRef, InstalledArtifact, InstalledArtifactRef, ManagedPackage,
    PackageCandidate, PackageCompatibility, PackageObservation, PackageSupportLevel,
};
pub use storage::{DataInodes, Inode, PackageKey, SlotView, UserId};
pub use transaction_token::{BootId, CommitNonce, TransactionId};
