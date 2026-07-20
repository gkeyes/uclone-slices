#![doc = "Append-only committed slot Registry."]

mod cache;
mod chain;
mod error;
mod revision;
mod store;
#[cfg(test)]
mod tests;

pub use error::RegistryError;
pub use revision::PackageRevision;
pub use store::RegistryStore;
