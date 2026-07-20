#![doc = "Typed fail-closed slot-switch runtime orchestration."]

mod backend;
mod coordinator;
mod error;
mod fault;
mod outcome;
mod request;
mod stores;

pub use backend::RuntimeBackend;
pub use coordinator::SwitchCoordinator;
pub use error::{PlatformError, RuntimeError};
pub use fault::{FaultInjector, FaultPoint};
pub use outcome::{RecoveryCause, SwitchOutcome};
pub use request::{SwitchMetadata, SwitchRequest};
pub use stores::RuntimeStores;
