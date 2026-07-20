#![doc = "Append-only Registry integrity and continuity tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

mod support;

#[path = "registry/cache.rs"]
mod cache;
#[path = "registry/chain.rs"]
mod chain;
#[path = "registry/integrity.rs"]
mod integrity;
#[path = "registry/storage_security.rs"]
mod storage_security;
