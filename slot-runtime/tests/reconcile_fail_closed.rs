#![doc = "Fail-closed reconciliation boundary and crash regressions."]
#![allow(clippy::unwrap_used, reason = "validated reconciliation test fixtures")]

#[path = "reconcile/support.rs"]
pub mod reconcile_support;

use std::fs;

use reconcile_support::{
    FakeBackend, TestStores, append_precommit, base_inodes, base_package, identity, publish_target,
    transaction,
};
use tempfile::TempDir;
use uclone_slot_runtime::domain::PackageObservation;
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionView};
use uclone_slot_runtime::reconcile::{
    ReconcileError, ReconcileOutcome, ReconcileReason, Reconciler,
};
use uclone_slot_runtime::registry::RegistryStore;

#[test]
fn clean_empty_enrollment_does_not_gate_allowlisted_package() {
    let root = TempDir::new().unwrap();
    reconcile_support::secure_temp_dir(&root);
    let managed = base_package();
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = Reconciler::new(
        backend,
        EnrollmentStore::new(root.path().join("enrollment")).unwrap(),
        JournalStore::new(root.path().join("journal")).unwrap(),
        RegistryStore::new(root.path().join("registry")).unwrap(),
    );

    let report = reconciler.early_boot().unwrap();

    assert!(report.results().is_empty());
    assert_eq!(reconciler.backend().emergency_gate_calls, 0);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn corrupt_enrollment_is_emergency_gated_before_error_without_ce_access() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let package_dir = stores
        .enrollment
        .root()
        .join("packages/com.uclone.slotprobe");
    fs::write(package_dir.join("unexpected"), b"corrupt").unwrap();
    let backend = FakeBackend::rebooted(&managed);
    let mut reconciler = stores.reconciler(backend);

    let error = reconciler.early_boot().unwrap_err();

    assert!(matches!(error, ReconcileError::Enrollment(_)));
    assert_eq!(reconciler.backend().emergency_gate_calls, 1);
    assert_eq!(reconciler.backend().quiesce_calls, 1);
    assert!(reconciler.backend().gate_held);
    assert_eq!(reconciler.backend().observe_calls, 0);
    assert_eq!(reconciler.backend().apply_calls, 0);
    assert_eq!(reconciler.backend().native_base_proofs, 0);
}

#[test]
fn emergency_gate_failure_aborts_early_boot() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.fail_emergency_gate = true;
    let mut reconciler = stores.reconciler(backend);

    let error = reconciler.early_boot().unwrap_err();

    assert!(matches!(error, ReconcileError::EmergencyGate { .. }));
    assert_eq!(reconciler.backend().observe_calls, 0);
    assert_eq!(reconciler.backend().apply_calls, 0);
}

#[test]
fn locked_metadata_uncertainty_is_not_masked_as_locked() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    fs::write(
        stores.journal.root().join("transactions/unexpected"),
        b"corrupt",
    )
    .unwrap();
    let mut backend = FakeBackend::rebooted(&managed);
    backend.unlocked = false;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::JournalMetadata),
    );
    assert!(reconciler.backend().gate_held);
}

#[test]
fn containment_failure_is_a_typed_error_not_a_recovery_outcome() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.observation = PackageObservation::new(
        identity("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        base_inodes(),
        base_inodes(),
        base_inodes(),
        false,
    );
    backend.fail_containment = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let error = reconciler.reconcile_unlocked().unwrap_err();

    assert!(matches!(error, ReconcileError::Containment { .. }));
}

#[test]
fn recovery_marker_failure_is_a_typed_error_not_a_recovery_outcome() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-marker-failure");
    append_precommit(&stores.journal, &pending);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.observation = PackageObservation::new(
        identity("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        base_inodes(),
        base_inodes(),
        base_inodes(),
        false,
    );
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();
    let steps = stores
        .journal
        .transaction_path(pending.transaction_id())
        .unwrap()
        .join("steps");
    fs::remove_dir_all(steps).unwrap();

    let error = reconciler.reconcile_unlocked().unwrap_err();

    assert!(matches!(error, ReconcileError::RecoveryMarker { .. }));
    assert!(reconciler.backend().gate_held);
}

#[test]
fn late_identity_drift_prevents_gate_release_after_view_apply() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let committed = transaction("tx-reconcile-late-drift");
    publish_target(&stores.registry, &committed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.late_observation = Some(PackageObservation::new(
        identity("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        base_inodes(),
        base_inodes(),
        base_inodes(),
        false,
    ));
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let report = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        report.results().first().unwrap().outcome(),
        &ReconcileOutcome::RecoveryRequired(ReconcileReason::PackageStateDrift),
    );
    assert_eq!(reconciler.backend().apply_calls, 1);
    assert!(reconciler.backend().gate_held);
}

#[test]
fn rollback_gate_released_crash_reproves_previous_before_completion() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let pending = transaction("tx-reconcile-rollback-gate-released");
    append_precommit(&stores.journal, &pending);
    for event in [
        JournalEvent::RollingBack,
        JournalEvent::RolledBack,
        JournalEvent::GateReleased,
    ] {
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
    assert!(matches!(
        completed.steps().last().unwrap().event(),
        JournalEvent::Completed
    ));
    assert_eq!(reconciler.backend().retire_gate_lease_calls, 1);
    assert!(!reconciler.backend().lease_present);
}
