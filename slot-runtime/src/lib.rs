#![doc = "Fail-closed transaction and package lifecycle core for `UClone` Slots Preview."]

pub(crate) mod atomic_file;
pub(crate) mod integrity;

#[doc = "Command-based Android runtime adapter and injected observation boundaries."]
pub mod android;
#[doc = "Strict fixed-argument protocol for the Android runtime information bridge."]
pub mod bridge;
#[doc = "Immutable, hash-protected package slot catalog metadata."]
pub mod catalog;
#[doc = "Restricted command-line client for the Preview control plane."]
pub mod cli;
#[doc = "Host-testable Unix socket server for the root runtime."]
pub mod daemon;
#[doc = "Validated package, slot, inode, and transaction boundary values."]
pub mod domain;
#[doc = "Immutable base package enrollment persistence."]
pub mod enrollment;
#[doc = "Durable pre-enrollment attempt, commit, and recovery anchors."]
pub mod enrollment_attempt;
#[doc = "Durable append-only transaction journal and state machine."]
pub mod journal;
#[doc = "Fixed Android Preview control-plane and slot path derivation."]
pub mod layout;
#[doc = "Fail-closed package update and inode-drift guard."]
pub mod lifecycle;
#[doc = "Fail-closed CE/DE data-slot materialization coordinator and platform boundary."]
pub mod materializer;
#[doc = "Append-only durable package lifecycle-state stream."]
pub mod package_state;
#[doc = "Production fixed-layout composition for the root Preview service."]
pub mod production;
#[doc = "Bounded versioned JSON-lines control-plane protocol."]
pub mod protocol;
#[doc = "Two-phase fail-closed reboot and user-unlock reconciliation."]
pub mod reconcile;
#[doc = "Deterministic crash-point reconciliation decisions."]
pub mod recovery;
#[doc = "Append-only active-slot commit Registry."]
pub mod registry;
#[doc = "Independent append-only emergency native-base rescue journal."]
pub mod rescue;
#[doc = "Typed slot-switch transaction coordinator and platform boundary."]
pub mod runtime;
#[doc = "Fixed-command daemon service composition for Slots Preview operations."]
pub mod service;
#[doc = "Generated immutable build-time target profile constants."]
pub mod target {
    include!(env!("UCLONE_TARGET_RS"));
}

mod store_security;
