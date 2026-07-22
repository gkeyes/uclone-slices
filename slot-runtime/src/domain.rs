#![doc = "Validated persistence and command-boundary values."]

mod error;
mod gate;
mod identifier;
mod package;
mod storage;
mod transaction_token;

pub use error::DomainError;
pub use gate::{GateSnapshot, PackageEnabledState};
pub use identifier::{PackageName, SlotId};
pub use package::{
    AppIdentity, ManagedPackage, PackageCandidate, PackageCompatibility, PackageObservation,
    PackageSupportLevel,
};
pub use storage::{DataInodes, Inode, PackageKey, SlotView, UserId};
pub use transaction_token::{BootId, CommitNonce, TransactionId};
