#![doc = "Durable Journal behavior and integrity tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

#[allow(dead_code)]
mod support;

#[path = "journal/storage_security.rs"]
mod storage_security;

use std::fs;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionSpec, TransactionView};

fn spec() -> TransactionSpec {
    let base = DataInodes::new(100, 200).unwrap();
    support::transaction_spec(support::TransactionFixture::new(
        "tx-00000001",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::base(), base),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(300, 400).unwrap(),
            ),
        ),
        "boot-00000001",
    ))
}

#[test]
fn appends_hash_chained_immutable_steps() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();

    store.create(&spec).unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();

    let transaction = store.load(spec.transaction_id()).unwrap();
    assert_eq!(transaction.steps().len(), 3);
    let first = transaction.steps().first().unwrap();
    let second = transaction.steps().get(1).unwrap();
    assert_eq!(first.generation(), 1);
    assert_eq!(second.generation(), 2);
    assert_eq!(second.previous_sha256(), Some(first.sha256()),);
    assert_eq!(transaction.view(), TransactionView::PreCommit);
}

#[test]
fn rejects_registry_commit_with_a_different_nonce() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::Applying)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::ViewVerified)
        .unwrap();
    store
        .append(
            spec.transaction_id(),
            JournalEvent::Committing {
                nonce: CommitNonce::parse("commit-00000001").unwrap(),
            },
        )
        .unwrap();

    let error = store
        .append(
            spec.transaction_id(),
            JournalEvent::RegistryCommitted {
                nonce: CommitNonce::parse("commit-99999999").unwrap(),
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains("nonce"));
}

#[test]
fn rejects_illegal_phase_transition() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();

    let error = store
        .append(spec.transaction_id(), JournalEvent::ViewVerified)
        .unwrap_err();

    assert!(error.to_string().contains("illegal journal transition"));
}

#[test]
fn rejects_corrupted_hash_chain() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    let second = store.step_path(spec.transaction_id(), 2).unwrap();
    let mut bytes = fs::read(&second).unwrap();
    let index = bytes.iter().position(|byte| *byte == b'g').unwrap();
    if let Some(byte) = bytes.get_mut(index) {
        *byte = b'G';
    }
    fs::write(&second, bytes).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(error.to_string().contains("digest"));
}

#[test]
fn ignores_uncommitted_temporary_step() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let temporary = store
        .transaction_path(spec.transaction_id())
        .unwrap()
        .join("steps/.0000000000000002.json.tmp-orphan");
    fs::write(&temporary, b"partial").unwrap();
    let mut permissions = fs::metadata(&temporary).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
    fs::set_permissions(temporary, permissions).unwrap();

    let transaction = store.load(spec.transaction_id()).unwrap();

    assert_eq!(transaction.steps().len(), 1);
}

#[test]
fn classifies_completed_rollback_as_previous_outcome() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::Applying)
        .unwrap();

    store
        .append(spec.transaction_id(), JournalEvent::RollingBack)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::RolledBack)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::GateReleased)
        .unwrap();
    store
        .append(spec.transaction_id(), JournalEvent::Completed)
        .unwrap();

    let transaction = store.load(spec.transaction_id()).unwrap();
    assert_eq!(transaction.view(), TransactionView::CompletedPrevious);
}
