#![doc = "Durable fixtures and deterministic adapters for offline-rescue tests."]
#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-local fixtures are re-exported across private test submodules"
)]

#[path = "support/backend.rs"]
mod backend;
#[path = "support/fault.rs"]
mod fault;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/metadata.rs"]
mod metadata;

pub(crate) use backend::FakeRescueBackend;
pub(crate) use fault::CrashOnce;
pub(crate) use fixture::{Fixture, SIGNATURE, base_inodes, identity};
pub(crate) use metadata::FakeMetadata;
