mod adapters;
mod composition;
mod model;
mod ports;
mod protocol;
mod usecases;

pub use composition::{CompositionError, ProductionRuntime, production};
pub use model::{
    BindingState, Capabilities, DisplayName, PackageName, PackageSnapshot, SeedMode,
    SigningIdentity, SigningKind, SlotId, SlotSnapshot,
};
pub use usecases::RuntimeError;

pub fn handle_line(runtime: &mut ProductionRuntime, line: &str) -> String {
    protocol::handle_line(runtime.inner_mut(), line)
}
