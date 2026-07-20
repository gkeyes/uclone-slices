#![doc = "Durable display and lifecycle metadata for immutable data-slot identifiers."]

mod error;
mod model;
mod store;
mod types;

pub use error::SlotMetadataError;
pub use model::SlotMetadata;
pub use store::SlotMetadataStore;
pub use types::{SlotDisplayName, SlotRecordState, SlotSeedMode};
