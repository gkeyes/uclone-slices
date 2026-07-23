#![doc = "Real coordinator crash-boundary matrix and durable recovery proofs."]
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "the matrix uses validated temporary-store fixtures and fail-fast proof branches"
)]

use super::fixture::{FakeBackend, coordinator, request};
use uclone_slot_runtime::journal::TransactionView;
use uclone_slot_runtime::recovery::{RecoveryDecision, decide_recovery};
use uclone_slot_runtime::registry::PackageRevision;
use uclone_slot_runtime::runtime::{FaultInjector, FaultPoint, RuntimeError};

#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each boolean is an independently asserted durable crash-boundary fact"
)]
struct CrashExpectation {
    point: FaultPoint,
    journal_view: TransactionView,
    registry_target: bool,
    current_target: bool,
    gate_held: bool,
    lease_retired: bool,
    recovery: RecoveryDecision,
}

const CRASH_MATRIX: [CrashExpectation; 13] = [
    CrashExpectation {
        point: FaultPoint::Prepared,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: false,
        gate_held: false,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::GateHeld,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: false,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::ProcessesQuiesced,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: false,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::Applying,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: false,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::TargetApplied,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: true,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::ViewVerified,
        journal_view: TransactionView::PreCommit,
        registry_target: false,
        current_target: true,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::Committing,
        journal_view: TransactionView::CommitPending,
        registry_target: false,
        current_target: true,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollbackToPrevious,
    },
    CrashExpectation {
        point: FaultPoint::RegistryPublished,
        journal_view: TransactionView::CommitPending,
        registry_target: true,
        current_target: true,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollForwardToTarget,
    },
    CrashExpectation {
        point: FaultPoint::RegistryCommitted,
        journal_view: TransactionView::PostCommit,
        registry_target: true,
        current_target: true,
        gate_held: true,
        lease_retired: false,
        recovery: RecoveryDecision::RollForwardToTarget,
    },
    CrashExpectation {
        point: FaultPoint::GateRestored,
        journal_view: TransactionView::PostCommit,
        registry_target: true,
        current_target: true,
        gate_held: false,
        lease_retired: false,
        recovery: RecoveryDecision::RollForwardToTarget,
    },
    CrashExpectation {
        point: FaultPoint::GateReleased,
        journal_view: TransactionView::CompletedTarget,
        registry_target: true,
        current_target: true,
        gate_held: false,
        lease_retired: false,
        recovery: RecoveryDecision::NoAction,
    },
    CrashExpectation {
        point: FaultPoint::Completed,
        journal_view: TransactionView::CompletedTarget,
        registry_target: true,
        current_target: true,
        gate_held: false,
        lease_retired: false,
        recovery: RecoveryDecision::NoAction,
    },
    CrashExpectation {
        point: FaultPoint::GateLeaseRetired,
        journal_view: TransactionView::CompletedTarget,
        registry_target: true,
        current_target: true,
        gate_held: false,
        lease_retired: true,
        recovery: RecoveryDecision::NoAction,
    },
];

#[test]
fn every_switch_coordinator_crash_point_preserves_a_bounded_recovery_proof() {
    for expected in CRASH_MATRIX {
        let request = request("tx-runtime-crash-matrix");
        let backend = FakeBackend::healthy(request.managed_package());
        let (_root, mut runtime) = coordinator(backend, FaultInjector::crash_at(expected.point));

        let error = runtime.switch(&request).unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::InjectedCrash(point) if point == expected.point
        ));

        let transaction = runtime
            .stores()
            .journal()
            .load(request.metadata().transaction_id())
            .unwrap();
        assert_eq!(
            transaction.view(),
            expected.journal_view,
            "journal view at {:?}",
            expected.point
        );

        let latest = runtime
            .stores()
            .registry()
            .latest(request.managed_package().package_name())
            .unwrap();
        match (expected.registry_target, latest.as_ref()) {
            (false, None) => {}
            (false, Some(revision)) => panic!(
                "unexpected Registry revision {:?} at {:?}",
                revision, expected.point
            ),
            (true, Some(revision)) => assert_target_revision(revision, &request),
            (true, None) => panic!("missing Registry target proof at {:?}", expected.point),
        }

        let current = runtime.backend().current.slot_id() == request.target_view().slot_id();
        assert_eq!(
            current, expected.current_target,
            "current view at {:?}",
            expected.point
        );
        assert_eq!(
            runtime.backend().gate_held,
            expected.gate_held,
            "gate state at {:?}",
            expected.point
        );
        assert_eq!(
            runtime.backend().lease_retired,
            expected.lease_retired,
            "lease state at {:?}",
            expected.point
        );
        assert_eq!(
            decide_recovery(&transaction, latest.as_ref()),
            expected.recovery,
            "recovery decision at {:?}",
            expected.point
        );
    }
}

fn assert_target_revision(
    revision: &PackageRevision,
    request: &uclone_slot_runtime::runtime::SwitchRequest,
) {
    let managed = request.managed_package();
    assert_eq!(revision.package_name(), managed.package_name());
    assert_eq!(revision.user_id(), managed.user_id());
    assert_eq!(revision.identity(), managed.identity());
    assert_eq!(revision.lifecycle_state(), managed.lifecycle_state());
    assert_eq!(revision.base_inodes(), managed.base_inodes());
    assert_eq!(
        revision.transaction_id(),
        request.metadata().transaction_id()
    );
    assert_eq!(revision.commit_nonce(), request.metadata().commit_nonce());
    assert_eq!(revision.active_slot(), request.target_view().slot_id());
    assert_eq!(revision.active_inodes(), request.target_view().inodes());
}
