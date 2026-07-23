#![doc = "Durable display and lifecycle metadata for immutable data-slot identifiers."]

mod epoch;
mod error;
mod model;
mod reader;
mod storage;
mod store;
mod stream;
mod types;

pub use error::SlotMetadataError;
pub use model::SlotMetadata;
pub use store::SlotMetadataStore;
pub use types::{SlotDisplayName, SlotRecordState, SlotSeedMode};
