#![allow(missing_docs)]

mod service_support;

use service_support::{Call, FailurePoint, FakePlatform, allowed, request};
use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::protocol::{Command, ErrorCode};
use uclone_slot_runtime::service::{PreviewService, ServiceError};

#[test]
fn prepublication_gate_failures_exact_restore_and_retire_attempt_anchor() {
    for failure in [FailurePoint::HoldGate, FailurePoint::Quiesce] {
        // Given
        let platform = FakePlatform::default().fail_at(failure, ServiceError::Internal);
        let mut service = PreviewService::new(platform);

        // When
        let response = service.handle(&request(Command::EnrollPackage { package: allowed() }));

        // Then
        assert_eq!(response.error_code(), Some(ErrorCode::Internal));
        assert!(service.platform().calls().contains(&Call::AbortEnrollment));
        assert!(!service.platform().enrollment_anchor());
        assert!(!service.platform().calls().contains(&Call::Enroll));
    }
}

#[test]
fn failed_prepublication_cleanup_leaves_discoverable_recovery_anchor() {
    // Given
    let platform = FakePlatform::default()
        .fail_at(FailurePoint::HoldGate, ServiceError::Internal)
        .fail_at(FailurePoint::AbortEnrollment, ServiceError::Internal);
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::EnrollPackage { package: allowed() }));

    // Then
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert!(service.platform().enrollment_anchor());
}
