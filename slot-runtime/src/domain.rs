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
pub use package::{AppIdentity, ManagedPackage, PackageCompatibility, PackageObservation};
pub use storage::{DataInodes, Inode, PackageKey, SlotView, UserId};
pub use transaction_token::{BootId, CommitNonce, TransactionId};
