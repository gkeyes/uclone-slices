#![doc = "Immutable slot metadata trusted by enrollment and switch preflight."]

mod error;
mod manifest;
mod model;
mod scan;
mod secure;
mod store;

pub use error::CatalogError;
pub use model::{CatalogEntry, PathSecurityProof, SecurityProfileProof};
pub use secure::VerifiedCatalogEntry;
pub use store::CatalogStore;
