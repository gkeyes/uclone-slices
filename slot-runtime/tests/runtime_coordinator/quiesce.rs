use super::fixture::{FailureMode, FakeBackend, coordinator, request};
use uclone_slot_runtime::domain::{ManagedPackage, PackageKey, SlotId, SlotView};
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::runtime::{FaultInjector, SwitchOutcome, SwitchRequest};

fn request_from_extension(transaction: &str) -> SwitchRequest {
    let base_request = request(transaction);
    let enrolled = base_request.managed_package();
    let active = base_request.target_view().clone();
    let managed = ManagedPackage::new(
        PackageKey::new(enrolled.package_name().clone(), enrolled.user_id()),
        enrolled.identity().clone(),
        enrolled.base_inodes(),
        active,
        enrolled.lifecycle_state(),
    )
    .unwrap();
    SwitchRequest::new(
        managed,
        SlotView::new(SlotId::base(), enrolled.base_inodes()),
        base_request.metadata().clone(),
    )
}

#[test]
fn quiesce_failure_from_extension_keeps_gate_without_mount_rollback() {
    let request = request_from_extension("tx-runtime-quiesce-failure");
    let previous = request.managed_package().active_slot().clone();
    let mut backend = FakeBackend::healthy(request.managed_package());
    backend.failure = FailureMode::Quiesce;
    let (_root, mut runtime) = coordinator(backend, FaultInjector::disabled());

    let outcome = runtime.switch(&request).unwrap();

    assert!(matches!(outcome, SwitchOutcome::RecoveryRequired { .. }));
    assert!(runtime.backend().gate_held);
    assert_eq!(runtime.backend().current.slot_id(), &previous);
    assert!(runtime.backend().mount_operations.is_empty());
    assert_eq!(runtime.backend().mutations, ["gate_acquired"]);
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
