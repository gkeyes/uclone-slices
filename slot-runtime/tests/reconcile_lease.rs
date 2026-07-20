#![doc = "Gate-lease crash-window and retirement regressions."]
#![allow(clippy::unwrap_used, reason = "validated reconciliation test fixtures")]

#[path = "reconcile/support.rs"]
pub mod reconcile_support;

use reconcile_support::{FakeBackend, TestStores, append_precommit, base_package, transaction};
use tempfile::TempDir;
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::reconcile::{
    ReconcileError, ReconcileOutcome, ReconcileReason, Reconciler,
};
use uclone_slot_runtime::registry::RegistryStore;

#[test]
fn crash_after_restore_before_gate_released_replays_and_retires_lease() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-lease-after-restore");
    append_precommit(&stores.journal, &pending);
    for event in [JournalEvent::RollingBack, JournalEvent::RolledBack] {
        stores
            .journal
            .append(pending.transaction_id(), event)
            .unwrap();
    }
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RolledBack,
    );
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert_eq!(completed.view(), TransactionView::CompletedPrevious);
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(!reconciler.backend().lease_present);
}

#[test]
fn crash_after_completed_before_retire_replays_clean_restore_then_retires() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let completed = transaction("tx-lease-after-completed");
    append_precommit(&stores.journal, &completed);
    for event in [
        JournalEvent::RollingBack,
        JournalEvent::RolledBack,
        JournalEvent::GateReleased,
        JournalEvent::Completed,
    ] {
        stores
            .journal
            .append(completed.transaction_id(), event)
            .unwrap();
    }
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();
    assert!(reconciler.backend().lease_present);

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RestoredBase,
    );
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(!reconciler.backend().lease_present);
}

#[test]
fn retire_failure_recontains_and_reports_held_without_rewriting_journal() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.fail_retire_gate_lease = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::GateLeaseRetirement),
    );
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(reconciler.backend().lease_present);
    assert!(reconciler.backend().gate_held);
}

#[test]
fn transaction_is_completed_before_failed_retire_recontains_package() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-lease-retire-failure");
    append_precommit(&stores.journal, &pending);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.fail_retire_gate_lease = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::GateLeaseRetirement),
    );
    let completed = stores.journal.load(pending.transaction_id()).unwrap();
    assert!(matches!(
        completed.steps().last().unwrap().event(),
        JournalEvent::Completed
    ));
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(reconciler.backend().lease_present);
    assert!(reconciler.backend().gate_held);
}

#[test]
fn valid_orphan_lease_is_tracked_and_kept_gated_without_enrollment() {
    let root = TempDir::new().unwrap();
    reconcile_support::secure_temp_dir(&root);
    let managed = base_package();
    let mut backend = FakeBackend::rebooted(&managed);
    backend.orphan_lease = true;
    let mut reconciler = Reconciler::new(
        backend,
        EnrollmentStore::new(root.path().join("enrollment")).unwrap(),
        JournalStore::new(root.path().join("journal")).unwrap(),
        RegistryStore::new(root.path().join("registry")).unwrap(),
    );

    let early = reconciler.early_boot().unwrap();

    assert_eq!(
        early.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata),
    );
    assert!(reconciler.backend().gate_held);
    assert!(reconciler.backend().lease_present);
    assert_eq!(reconciler.backend().observe_calls, 0);

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata),
    );
    assert!(reconciler.backend().gate_held);
    assert_eq!(reconciler.backend().emergency_gate_calls, 2);
}

#[test]
fn corrupt_orphan_lease_is_gated_before_typed_early_boot_error() {
    let root = TempDir::new().unwrap();
    reconcile_support::secure_temp_dir(&root);
    let managed = base_package();
    let mut backend = FakeBackend::rebooted(&managed);
    backend.corrupt_orphan_lease = true;
    let mut reconciler = Reconciler::new(
        backend,
        EnrollmentStore::new(root.path().join("enrollment")).unwrap(),
        JournalStore::new(root.path().join("journal")).unwrap(),
        RegistryStore::new(root.path().join("registry")).unwrap(),
    );

    let error = reconciler.early_boot().unwrap_err();

    assert!(matches!(error, ReconcileError::EmergencyGate { .. }));
    assert!(reconciler.backend().gate_held);
    assert!(reconciler.backend().lease_present);
    assert_eq!(reconciler.backend().quiesce_calls, 1);
    assert_eq!(reconciler.backend().observe_calls, 0);
}
