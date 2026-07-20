#![doc = "Independent offline-rescue orchestration and fail-closed tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated host fixtures must abort the individual test on failure"
)]

#[path = "offline_rescue/drift.rs"]
mod drift;
#[path = "offline_rescue/isolation.rs"]
mod isolation;
#[path = "offline_rescue/resume.rs"]
mod resume;
#[path = "offline_rescue/support.rs"]
mod support;
