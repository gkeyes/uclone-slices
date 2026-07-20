#![doc = "End-to-end reboot and unlock reconciliation scenarios."]
#![allow(clippy::unwrap_used, reason = "validated reconciliation test fixtures")]

#[path = "reconcile/support.rs"]
pub mod reconcile_support;

use reconcile_support::{
    FakeBackend, TestStores, append_commit_pending, append_precommit, base_package, publish_target,
    transaction, work_view,
};
use uclone_slot_runtime::domain::{CommitNonce, PackageObservation, SlotId};
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::reconcile::{ReconcileOutcome, ReconcileReason};

#[test]
fn restores_native_base_without_bind_after_unlock() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);

    let early = reconciler.early_boot().unwrap();

    assert_eq!(
        early.results().first().unwrap().outcome(),
        &ReconcileOutcome::Held,
    );
    assert_eq!(reconciler.backend().observe_calls, 0);
    assert_eq!(reconciler.backend().apply_calls, 0);
    assert_eq!(reconciler.backend().native_base_proofs, 0);

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RestoredBase,
    );
    assert_eq!(reconciler.backend().native_base_proofs, 2);
    assert_eq!(reconciler.backend().apply_calls, 0);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn reapplies_and_verifies_committed_non_base_view_after_unlock() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let committed = transaction("tx-reconcile-clean-slot");
    publish_target(&stores.registry, &committed);
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RestoredSlot(SlotId::parse("work").unwrap()),
    );
    assert_eq!(reconciler.backend().current, work_view());
    assert_eq!(reconciler.backend().apply_calls, 1);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn rolls_back_precommit_transaction_before_restoring_gate() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-precommit");
    append_precommit(&stores.journal, &pending);
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledBack,
    );
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(completed.view(), TransactionView::CompletedPrevious);
    assert_eq!(reconciler.backend().native_base_proofs, 2);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn rolls_back_a_platform_failure_marker_before_commit() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-platform-failure");
    stores.journal.create(&pending).unwrap();
    for event in [
        JournalEvent::GateHeld,
        JournalEvent::ProcessesQuiesced,
        JournalEvent::RecoveryRequired {
            reason: "platform_failure".to_owned(),
        },
    ] {
        stores
            .journal
            .append(pending.transaction_id(), event)
            .unwrap();
    }
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledBack,
    );
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(completed.view(), TransactionView::CompletedPrevious);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn rolls_forward_when_registry_won_commit_pending_race() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-forward");
    append_commit_pending(&stores.journal, &pending);
    publish_target(&stores.registry, &pending);
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledForward,
    );
    assert_eq!(reconciler.backend().current, work_view());
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(completed.view(), TransactionView::CompletedTarget);
}

#[test]
fn rolls_back_commit_pending_only_with_registry_previous_proof() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-commit-back");
    append_commit_pending(&stores.journal, &pending);
    assert!(
        stores
            .journal
            .append(pending.transaction_id(), JournalEvent::RollingBack)
            .is_err()
    );
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledBack,
    );
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(completed.view(), TransactionView::CompletedPrevious);
}

#[test]
fn completes_gate_released_target_only_after_reproving_view() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-gate-released");
    append_commit_pending(&stores.journal, &pending);
    publish_target(&stores.registry, &pending);
    stores
        .journal
        .append(
            pending.transaction_id(),
            JournalEvent::RegistryCommitted {
                nonce: CommitNonce::parse("nonce-reconcile-0001").unwrap(),
            },
        )
        .unwrap();
    stores
        .journal
        .append(pending.transaction_id(), JournalEvent::GateReleased)
        .unwrap();
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledForward,
    );
    assert_eq!(reconciler.backend().apply_calls, 1);
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert!(matches!(
        completed.steps().last().unwrap().event(),
        JournalEvent::Completed
    ));
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(!reconciler.backend().lease_present);
}

#[test]
fn quarantines_identity_mismatch_and_records_recovery_required() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-identity");
    append_precommit(&stores.journal, &pending);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.observation = PackageObservation::new(
        reconcile_support::identity(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
        managed.base_inodes(),
        managed.base_inodes(),
        managed.base_inodes(),
        false,
    );
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::Quarantined,
    );
    assert!(reconciler.backend().gate_held);
    let transaction = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(transaction.view(), TransactionView::RecoveryRequired);
}

#[test]
fn locked_user_never_touches_ce_or_releases_gate() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.unlocked = false;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let locked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        locked.results().first().unwrap().outcome(),
        &ReconcileOutcome::Locked,
    );
    assert_eq!(reconciler.backend().observe_calls, 0);
    assert_eq!(reconciler.backend().native_base_proofs, 0);
    assert_eq!(reconciler.backend().apply_calls, 0);
    assert!(reconciler.backend().gate_held);
}

#[test]
fn gate_restore_failure_reacquires_gate_and_fails_closed() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-gate-failure");
    append_precommit(&stores.journal, &pending);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.fail_gate_restore = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::GateRestoreFailed),
    );
    assert!(reconciler.backend().gate_held);
    let transaction = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(transaction.view(), TransactionView::RecoveryRequired);
}
