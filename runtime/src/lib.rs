mod adapters;
pub mod archive;
mod composition;
pub mod layout;
mod links;
mod model;
mod ports;
mod protocol;
mod usecases;
pub mod validation;

pub use composition::{CompositionError, ProductionRuntime, production};
pub use usecases::RuntimeError;

pub fn handle_line(runtime: &mut ProductionRuntime, expected_build_id: &str, line: &str) -> String {
    protocol::handle_line(runtime.inner_mut(), expected_build_id, line)
}
