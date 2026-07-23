#![doc = "Integration coverage for the typed switch transaction coordinator."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

#[path = "runtime_coordinator/fixture.rs"]
mod fixture;
#[path = "runtime_coordinator/gate_acquire.rs"]
mod gate_acquire;
#[path = "runtime_coordinator/quiesce.rs"]
mod quiesce;
#[allow(dead_code)]
mod support;

use fixture::{FailureMode, FakeBackend, coordinator, request};
use uclone_slot_runtime::domain::{DataInodes, PackageObservation, SlotId};
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::lifecycle::GuardDecision;
use uclone_slot_runtime::runtime::{FaultInjector, FaultPoint, RuntimeError, SwitchOutcome};

#[test]
fn commits_verified_target_and_releases_exact_gate() {
    let request = request("tx-runtime-success");
    let backend = FakeBackend::healthy(request.managed_package());
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());
    let outcome = runtime.switch(&request).unwrap();
    assert!(matches!(outcome, SwitchOutcome::Committed { .. }));
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::CompletedTarget);
    assert!(!runtime.backend().gate_held);
    assert!(runtime.backend().lease_retired);
}

#[test]
fn target_verification_failure_rolls_back_and_restores_gate() {
    let request = request("tx-runtime-rollback");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::TargetVerification;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());
    let outcome = runtime.switch(&request).unwrap();

    assert!(matches!(outcome, SwitchOutcome::RolledBack { .. }));
    assert_eq!(runtime.backend().current.slot_id(), &SlotId::base());
    assert!(!runtime.backend().gate_held);
    assert!(runtime.backend().lease_retired);
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::CompletedPrevious);
}

#[test]
fn crash_after_registry_publish_leaves_commit_pending_and_gate_held() {
    let request = request("tx-runtime-published");
    let backend = FakeBackend::healthy(request.managed_package());
    let (_root, mut runtime) = coordinator(
        backend,
        FaultInjector::crash_at(FaultPoint::RegistryPublished),
    );

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::InjectedCrash(FaultPoint::RegistryPublished)
    ));
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::CommitPending);
    let latest = runtime
        .stores()
        .registry()
        .latest(request.managed_package().package_name())
        .unwrap()
        .unwrap();
    assert_eq!(latest.active_slot(), request.target_view().slot_id());
    assert!(runtime.backend().gate_held);
}

#[test]
fn guard_rejection_has_no_journal_or_backend_mutation() {
    let request = request("tx-runtime-rejected");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.observation = PackageObservation::new(
        backend.observation.identity().clone(),
        DataInodes::new(999, 1_000).unwrap(),
        request.managed_package().active_inodes(),
        request.managed_package().active_inodes(),
        false,
    );
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::GuardRejected(GuardDecision::RecoveryRequired(_))
    ));
    assert!(runtime.backend().mutations.is_empty());
    assert!(matches!(
        runtime
            .stores()
            .journal()
            .load(request.metadata().transaction_id()),
        Err(uclone_slot_runtime::journal::JournalError::Io { .. })
    ));
}

#[test]
fn failed_recovery_containment_is_an_explicit_error() {
    let request = request("tx-runtime-containment");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::RollbackAndContainment;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(error, RuntimeError::ContainmentFailed { .. }));
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert!(matches!(
        transaction
            .steps()
            .last()
            .map(uclone_slot_runtime::journal::JournalStep::event),
        Some(JournalEvent::RollingBack)
    ));
}

#[test]
fn success_journal_contains_every_required_boundary() {
    let request = request("tx-runtime-events");
    let backend = FakeBackend::healthy(request.managed_package());
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());
    runtime.switch(&request).unwrap();
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    let mut events = transaction
        .steps()
        .iter()
        .map(uclone_slot_runtime::journal::JournalStep::event);

    assert!(matches!(events.next(), Some(JournalEvent::Prepared { .. })));
    assert!(matches!(events.next(), Some(JournalEvent::GateHeld)));
    assert!(matches!(
        events.next(),
        Some(JournalEvent::ProcessesQuiesced)
    ));
    assert!(matches!(events.next(), Some(JournalEvent::Applying)));
    assert!(matches!(events.next(), Some(JournalEvent::ViewVerified)));
    assert!(matches!(
        events.next(),
        Some(JournalEvent::Committing { .. })
    ));
    assert!(matches!(
        events.next(),
        Some(JournalEvent::RegistryCommitted { .. })
    ));
    assert!(matches!(events.next(), Some(JournalEvent::GateReleased)));
    assert!(matches!(events.next(), Some(JournalEvent::Completed)));
    assert_eq!(events.next(), None);
}

#[test]
fn lease_retirement_failure_reacquires_gate_after_completed_view() {
    let request = request("tx-runtime-retire-fail");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::GateLeaseRetirement;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let outcome = runtime.switch(&request).unwrap();

    assert!(matches!(outcome, SwitchOutcome::RecoveryRequired { .. }));
    assert!(runtime.backend().gate_held);
    assert!(!runtime.backend().lease_retired);
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::CompletedTarget);
}

#[test]
fn crash_after_completed_retains_lease_on_a_complete_target_view() {
    let request = request("tx-runtime-completed-crash");
    let backend = FakeBackend::healthy(request.managed_package());
    let (_root, mut runtime) = coordinator(backend, FaultInjector::crash_at(FaultPoint::Completed));

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::InjectedCrash(FaultPoint::Completed)
    ));
    assert!(!runtime.backend().gate_held);
    assert!(!runtime.backend().lease_retired);
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::CompletedTarget);
}
