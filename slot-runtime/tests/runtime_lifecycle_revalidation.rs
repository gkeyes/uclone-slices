#![doc = "Late package-lifecycle proof before any slot mount mutation."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

#[path = "runtime_coordinator/fixture.rs"]
mod fixture;
#[allow(dead_code)]
mod support;

use fixture::{FakeBackend, coordinator, request};
use uclone_slot_runtime::domain::{AppIdentity, DataInodes, PackageObservation};
use uclone_slot_runtime::journal::TransactionView;
use uclone_slot_runtime::lifecycle::GuardDecision;
use uclone_slot_runtime::runtime::{FaultInjector, RuntimeError};

#[test]
fn identity_drift_after_quiescence_never_reaches_mount_mutation() {
    let request = request("tx-runtime-late-identity");
    let mut backend = FakeBackend::healthy(request.managed_package());
    let observed = &backend.observation;
    let changed_identity = AppIdentity::new(
        observed.identity().uid(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        observed.identity().version_code(),
        observed.identity().code_path(),
    )
    .unwrap();
    backend.late_observation = Some(PackageObservation::new(
        changed_identity,
        observed.package_manager_inodes(),
        observed.canonical_inodes(),
        observed.active_process_inodes(),
        false,
    ));
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::GuardRejected(GuardDecision::Quarantine)
    ));
    assert!(runtime.backend().gate_held);
    assert!(!runtime.backend().mutations.contains(&"view_applied"));
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::RecoveryRequired);
}

#[test]
fn inode_drift_after_quiescence_is_durable_recovery_required() {
    let request = request("tx-runtime-late-inodes");
    let mut backend = FakeBackend::healthy(request.managed_package());
    let observed = &backend.observation;
    backend.late_observation = Some(PackageObservation::new(
        observed.identity().clone(),
        DataInodes::new(901, 902).unwrap(),
        observed.canonical_inodes(),
        observed.active_process_inodes(),
        false,
    ));
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::GuardRejected(GuardDecision::RecoveryRequired(_))
    ));
    assert!(runtime.backend().gate_held);
    assert!(!runtime.backend().mutations.contains(&"view_applied"));
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::RecoveryRequired);
}
