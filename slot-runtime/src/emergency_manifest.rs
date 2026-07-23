#![doc = "Independent boot-time containment manifest and fail-closed durable store."]

mod error;
mod lock;
mod model;
mod storage;
mod store;
mod types;

pub use error::EmergencyManifestError;
pub use model::EmergencyManifestV1;
pub use store::{EmergencyManifestStore, MANIFEST_FILE_NAME};
pub use types::{ContainmentObligation, OverallDisposition, PackageContainment, SCHEMA_VERSION};

pub(super) const fn no_follow_flag() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x0000_0100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x0002_0000
    }
}
