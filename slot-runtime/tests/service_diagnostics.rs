#![allow(missing_docs)]
#![allow(clippy::unwrap_used, reason = "recording sink test fixture")]

use std::sync::{Arc, Mutex};

mod service_support;

use service_support::{FakePlatform, allowed, preview_view, request};
use uclone_slot_runtime::daemon::{MutationGuard, RequestHandler};
use uclone_slot_runtime::protocol::{Command, ErrorCode};
use uclone_slot_runtime::service::{
    CapabilitySnapshot, DiagnosticCause, DiagnosticCommand, DiagnosticFailure, DiagnosticSink,
    OperationPhase, PreviewService, ServiceError,
};
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotSeedMode};

#[derive(Clone, Debug, Default)]
struct RecordingSink {
    failures: Arc<Mutex<Vec<DiagnosticFailure>>>,
}

impl DiagnosticSink for RecordingSink {
    fn record_failure(&self, failure: DiagnosticFailure) {
        self.failures.lock().unwrap().push(failure);
    }
}

impl RecordingSink {
    fn failures(&self) -> Vec<DiagnosticFailure> {
        self.failures.lock().unwrap().clone()
    }
}

#[test]
fn service_error_code_stays_wire_compatible_while_failure_records_two_axes() {
    let sink = RecordingSink::default();
    let mut service = PreviewService::with_diagnostic_sink(FakePlatform::default(), sink.clone());
    let package = allowed();

    let response = service.handle(&request(Command::StatusPackage {
        package: package.clone(),
    }));

    assert_eq!(response.error_code(), Some(ErrorCode::NotFound));
    let failures = sink.failures();
    assert_eq!(failures.len(), 1);
    let failure = &failures[0];
    assert_eq!(failure.cause(), DiagnosticCause::Unknown);
    assert_eq!(failure.service_error(), ServiceError::NotFound);
    assert_eq!(failure.context().request_id().as_str(), "service-test");
    assert_eq!(
        failure.context().command(),
        DiagnosticCommand::StatusPackage
    );
    assert_eq!(failure.context().package(), Some(&package));
    assert_eq!(failure.context().slot(), None);
    assert_eq!(failure.context().phase(), OperationPhase::Dispatch);
}

#[test]
fn busy_error_records_lock_conflict_at_mutation_gate() {
    let sink = RecordingSink::default();
    let guard = MutationGuard::new();
    let permit = guard.try_acquire().unwrap();
    let mut service = PreviewService::with_mutation_guard_and_diagnostic_sink(
        FakePlatform::default(),
        guard.clone(),
        sink.clone(),
    );

    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));

    assert_eq!(response.error_code(), Some(ErrorCode::Busy));
    let failure = sink.failures().pop().unwrap();
    assert_eq!(failure.cause(), DiagnosticCause::LockConflict);
    assert_eq!(failure.context().phase(), OperationPhase::MutationGate);
    drop(permit);
}

#[test]
fn recovery_only_rejection_records_context_without_changing_code() {
    let sink = RecordingSink::default();
    let mut service =
        PreviewService::with_recovery_only_diagnostic_sink(FakePlatform::default(), sink.clone());
    let response = service.handle(&request(Command::CreateSlot {
        package: allowed(),
        display_name: SlotDisplayName::parse("test").unwrap(),
        seed_mode: SlotSeedMode::Blank,
    }));

    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    let failure = sink.failures().pop().unwrap();
    assert_eq!(failure.cause(), DiagnosticCause::Unknown);
    assert_eq!(failure.context().phase(), OperationPhase::RecoveryGate);
    assert_eq!(failure.context().command(), DiagnosticCommand::CreateSlot);
}

#[test]
fn unsupported_capability_records_policy_cause_at_capability_phase() {
    let sink = RecordingSink::default();
    let platform =
        FakePlatform::default().with_capability(CapabilitySnapshot::new(false, true, true, false));
    let mut service = PreviewService::with_diagnostic_sink(platform, sink.clone());
    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));

    assert_eq!(response.error_code(), Some(ErrorCode::UnsupportedDevice));
    let failure = sink.failures().pop().unwrap();
    assert_eq!(failure.cause(), DiagnosticCause::CapabilityPolicy);
    assert_eq!(failure.context().phase(), OperationPhase::CapabilityCheck);
}
