use super::support;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionSpec};
use uclone_slot_runtime::recovery::{RecoveryDecision, decide_recovery};
use uclone_slot_runtime::registry::{PackageRevision, RegistryStore};

fn spec() -> TransactionSpec {
    let base = DataInodes::new(101, 201).unwrap();
    support::transaction_spec(support::TransactionFixture::new(
        "tx-00000002",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::base(), base),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
        ),
        "boot-00000002",
    ))
}

#[test]
fn rolls_back_when_registry_commit_did_not_happen() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let spec = spec();
    journal.create(&spec).unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();

    let transaction = journal.load(spec.transaction_id()).unwrap();
    let latest = registry.latest(spec.package_name()).unwrap();
    let decision = decide_recovery(&transaction, latest.as_ref());

    assert_eq!(decision, RecoveryDecision::RollbackToPrevious);
}

#[test]
fn rolls_forward_when_registry_commit_won_the_crash_race() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let spec = spec();
    journal.create(&spec).unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::Applying)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::ViewVerified)
        .unwrap();
    journal
        .append(
            spec.transaction_id(),
            JournalEvent::Committing {
                nonce: CommitNonce::parse("commit-00000002").unwrap(),
            },
        )
        .unwrap();
    registry
        .append(
            &PackageRevision::committed(
                &spec,
                spec.previous_inodes(),
                CommitNonce::parse("commit-00000002").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(spec.transaction_id()).unwrap();
    let latest = registry.latest(spec.package_name()).unwrap().unwrap();
    let decision = decide_recovery(&transaction, Some(&latest));

    assert_eq!(decision, RecoveryDecision::RollForwardToTarget);
}

#[test]
fn requires_manual_recovery_when_registry_nonce_conflicts() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let spec = spec();
    journal.create(&spec).unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::Applying)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::ViewVerified)
        .unwrap();
    journal
        .append(
            spec.transaction_id(),
            JournalEvent::Committing {
                nonce: CommitNonce::parse("commit-00000002").unwrap(),
            },
        )
        .unwrap();
    registry
        .append(
            &PackageRevision::committed(
                &spec,
                spec.previous_inodes(),
                CommitNonce::parse("different-nonce").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(spec.transaction_id()).unwrap();
    let latest = registry.latest(spec.package_name()).unwrap().unwrap();
    let decision = decide_recovery(&transaction, Some(&latest));

    assert_eq!(decision, RecoveryDecision::RecoveryRequired);
}

#[test]
fn rolls_back_a_proved_platform_failure_before_commit() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let spec = spec();
    journal.create(&spec).unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    journal
        .append(spec.transaction_id(), JournalEvent::ProcessesQuiesced)
        .unwrap();
    journal
        .append(
            spec.transaction_id(),
            JournalEvent::RecoveryRequired {
                reason: "platform_failure".to_owned(),
            },
        )
        .unwrap();

    let transaction = journal.load(spec.transaction_id()).unwrap();
    let latest = registry.latest(spec.package_name()).unwrap();

    assert_eq!(
        decide_recovery(&transaction, latest.as_ref()),
        RecoveryDecision::RollbackToPrevious
    );
}
