use super::fixture::{FailureMode, FakeBackend, coordinator, request};
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::runtime::{FaultInjector, RuntimeError, SwitchOutcome};

#[test]
fn acquire_ack_loss_is_durably_contained_without_mounting() {
    let request = request("tx-runtime-gate-ack-lost");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::GateAcquireAfterMutation;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let outcome = runtime.switch(&request).unwrap();

    assert!(matches!(outcome, SwitchOutcome::RecoveryRequired { .. }));
    assert!(runtime.backend().gate_held);
    assert_eq!(
        runtime.backend().mutations,
        ["gate_acquired", "processes_quiesced"]
    );
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::RecoveryRequired);
    assert!(matches!(
        transaction
            .steps()
            .last()
            .map(uclone_slot_runtime::journal::JournalStep::event),
        Some(JournalEvent::RecoveryRequired { reason }) if reason == "platform_failure"
    ));
}

#[test]
fn unknown_initial_gate_failure_is_containment_failed_without_mounting() {
    let request = request("tx-runtime-gate-unknown");
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::GateAcquire;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let error = runtime.switch(&request).unwrap_err();

    assert!(matches!(error, RuntimeError::ContainmentFailed { .. }));
    assert!(!runtime.backend().gate_held);
    assert!(runtime.backend().mutations.is_empty());
    let transaction = runtime
        .stores()
        .journal()
        .load(request.metadata().transaction_id())
        .unwrap();
    assert_eq!(transaction.view(), TransactionView::PreCommit);
}
