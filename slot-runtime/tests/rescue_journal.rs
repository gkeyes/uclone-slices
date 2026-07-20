#![doc = "Emergency rescue-journal state-machine and resume tests."]
#![allow(
    clippy::unwrap_used,
    reason = "fixture construction must abort the individual test on failure"
)]

#[path = "rescue_journal/integrity.rs"]
mod integrity;
#[path = "rescue_journal/support.rs"]
mod support;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::rescue::{RescueEvent, RescueStatus};

#[test]
fn persists_strict_chain_through_base_retirement() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    let prepared = store.begin(&spec).unwrap();
    assert_eq!(prepared.status(), RescueStatus::PreCommit);

    support::append_until_verified(&store, &spec);
    let committed = store
        .append(
            spec.rescue_id(),
            RescueEvent::BaseCommitted {
                nonce: spec.commit_nonce().clone(),
            },
        )
        .unwrap();
    assert_eq!(committed.generation(), 6);
    assert_eq!(
        store.load().unwrap().unwrap().status(),
        RescueStatus::Committed
    );

    store
        .append(spec.rescue_id(), RescueEvent::GateReleased)
        .unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().status(),
        RescueStatus::Completed
    );
    store
        .append(spec.rescue_id(), RescueEvent::Completed)
        .unwrap();
    let retired = store.load().unwrap().unwrap();
    assert_eq!(retired.status(), RescueStatus::BaseRetired);
    for pair in retired.steps().windows(2) {
        let [previous, next] = pair else {
            continue;
        };
        assert_eq!(next.previous_sha256(), Some(previous.sha256()));
    }
}

#[test]
fn failure_retains_proven_phase_and_exact_spec_resumes() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    store
        .append(
            spec.rescue_id(),
            RescueEvent::Failure {
                code: "gate_command_failed".to_owned(),
            },
        )
        .unwrap();

    assert_eq!(
        store.begin(&spec).unwrap().status(),
        RescueStatus::PreCommit
    );
    store
        .append(spec.rescue_id(), RescueEvent::GateHeld)
        .unwrap();
    assert_eq!(store.load().unwrap().unwrap().steps().len(), 3);
}

#[test]
fn rejects_different_snapshot_and_identity_for_existing_epoch() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    let changed_gate =
        support::spec_with_gate(GateSnapshot::new(PackageEnabledState::DisabledUser, true));
    assert!(
        store
            .begin(&changed_gate)
            .unwrap_err()
            .to_string()
            .contains("different")
    );

    let changed_identity = support::spec_with_version(2);
    assert!(
        store
            .begin(&changed_identity)
            .unwrap_err()
            .to_string()
            .contains("different")
    );
}

#[test]
fn rejects_illegal_transition_commit_nonce_and_failure_code() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    let illegal = store
        .append(spec.rescue_id(), RescueEvent::BaseApplying)
        .unwrap_err();
    assert!(illegal.to_string().contains("illegal rescue transition"));

    support::append_until_verified(&store, &spec);
    let wrong_nonce = store
        .append(
            spec.rescue_id(),
            RescueEvent::BaseCommitted {
                nonce: CommitNonce::parse("nonce-wrong-0001").unwrap(),
            },
        )
        .unwrap_err();
    assert!(wrong_nonce.to_string().contains("nonce"));
    let invalid_code = store
        .append(
            spec.rescue_id(),
            RescueEvent::Failure {
                code: "Not Machine Safe".to_owned(),
            },
        )
        .unwrap_err();
    assert!(invalid_code.to_string().contains("failure code"));
}

#[test]
fn bounds_failure_generations_without_advancing_phase() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    for _ in 1..32 {
        store
            .append(
                spec.rescue_id(),
                RescueEvent::Failure {
                    code: "retryable".to_owned(),
                },
            )
            .unwrap();
    }
    let error = store
        .append(
            spec.rescue_id(),
            RescueEvent::Failure {
                code: "retryable".to_owned(),
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("bound exceeded"));
}
