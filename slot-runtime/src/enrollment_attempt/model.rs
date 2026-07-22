#[path = "anchors.rs"]
mod anchors;
#[path = "phase.rs"]
mod phase;
#[path = "proof.rs"]
mod proof;
#[path = "record.rs"]
mod record;

pub use anchors::CommittedAnchors;
pub use phase::EnrollmentAttemptPhase;
pub use proof::{CommitProof, RetirementProof};
pub use record::EnrollmentAttempt;

pub(super) const SCHEMA_VERSION: u32 = 2;
