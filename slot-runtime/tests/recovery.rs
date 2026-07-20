#![doc = "Journal and Registry crash-point reconciliation tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

mod support;

#[path = "recovery/decisions.rs"]
mod decisions;
#[path = "recovery/identity.rs"]
mod identity;
