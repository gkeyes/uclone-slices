#![doc = "Durable fail-closed enrollment attempt and commit anchors."]

mod error;
mod model;
mod storage;
mod store;
#[cfg(test)]
mod tests;

pub use error::EnrollmentAttemptError;
pub use model::{
    CommitProof, CommittedAnchors, EnrollmentAttempt, EnrollmentAttemptPhase, RetirementProof,
};
pub use store::EnrollmentAttemptStore;
