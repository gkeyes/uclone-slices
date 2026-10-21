mod adapters;
pub mod archive;
mod composition;
pub mod launch;
pub mod layout;
mod links;
mod model;
mod ports;
mod protocol;
mod usecases;
pub mod validation;

pub use composition::{
    CompositionError, ProductionRuntime, production, production_with_launch_gate,
};
pub use usecases::RuntimeError;

/// Identity compiled into the executable, independent of module metadata or launch environment.
pub const BINARY_BUILD_ID: &str = env!("CARGO_PKG_VERSION");

pub fn handle_line(runtime: &mut ProductionRuntime, expected_build_id: &str, line: &str) -> String {
    protocol::handle_line(runtime.inner_mut(), expected_build_id, line)
}
