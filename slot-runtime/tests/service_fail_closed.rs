#![allow(missing_docs)]

mod service_support;

use service_support::{
    Call, FailurePoint, FakePlatform, allowed, preview_view, ready_base, request,
};
use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::protocol::{Command, ErrorCode, ReconcileOutcome, ResponsePayload};
use uclone_slot_runtime::service::{PackageState, PreviewService, RescueExecution, ServiceError};

#[test]
fn ambiguous_and_quarantined_state_return_stable_codes_without_mutation() {
    for (state, expected) in [
        (PackageState::RecoveryRequired, ErrorCode::RecoveryRequired),
        (PackageState::Quarantined, ErrorCode::Quarantined),
    ] {
        // Given
        let mut service = PreviewService::new(FakePlatform::with_state(state));

        // When
        let response = service.handle(&request(Command::Switch {
            package: allowed(),
            slot: preview_view().slot_id().clone(),
        }));

        // Then
        assert_eq!(response.error_code(), Some(expected));
        assert_eq!(service.platform().calls(), vec![Call::State]);
    }
}

#[test]
fn absent_enrollment_is_not_confused_with_corrupt_missing_state() {
    // Given
    let mut absent = PreviewService::new(FakePlatform::default());
    let mut corrupt = PreviewService::new(FakePlatform::with_state(PackageState::RecoveryRequired));

    // When
    let absent_response = absent.handle(&request(Command::StatusPackage { package: allowed() }));
    let corrupt_response = corrupt.handle(&request(Command::StatusPackage { package: allowed() }));

    // Then
    assert_eq!(absent_response.error_code(), Some(ErrorCode::NotFound));
    assert_eq!(
        corrupt_response.error_code(),
        Some(ErrorCode::RecoveryRequired)
    );
}

#[test]
fn unpublished_enrollment_failure_exact_aborts_and_preserves_cause() {
    // Given
    let platform = FakePlatform::default().fail_at(FailurePoint::Enroll, ServiceError::Internal);
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));

    // Then
    assert_eq!(response.error_code(), Some(ErrorCode::Internal));
    assert!(service.platform().calls().contains(&Call::AbortEnrollment));
    assert!(!service.platform().enrollment_anchor());
}

#[test]
fn ambiguous_enrollment_publication_retains_recovery_anchor() {
    // Given
    let platform =
        FakePlatform::default().fail_at(FailurePoint::EnrollAmbiguous, ServiceError::Internal);
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));

    // Then
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert!(!service.platform().calls().contains(&Call::AbortEnrollment));
    assert!(service.platform().enrollment_anchor());
}

#[test]
fn enrollment_lease_retirement_failure_recontains_before_recovery() {
    // Given
    let platform =
        FakePlatform::default().fail_at(FailurePoint::RetireGate, ServiceError::Internal);
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));

    // Then
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::State,
            Call::Inspect,
            Call::BeginEnrollment,
            Call::HoldGate,
            Call::Quiesce,
            Call::Enroll(false),
            Call::ProveBase,
            Call::RestoreGate,
            Call::RetireGate,
            Call::HoldGate,
            Call::Quiesce,
            Call::MarkRecovery,
        ]
    );
    assert!(service.platform().enrollment_anchor());
}

#[test]
fn materialization_failure_never_runs_switch_or_restores_gate() {
    // Given
    let platform = FakePlatform::with_state(ready_base())
        .fail_at(FailurePoint::Materialize, ServiceError::UserLocked);
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));

    // Then
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::State,
            Call::CaptureGate,
            Call::HoldGate,
            Call::Quiesce,
            Call::Materialize,
        ]
    );
}

#[test]
fn rescue_rejects_an_unproved_offline_recovery() {
    let platform = FakePlatform::with_state(PackageState::RecoveryRequired)
        .with_rescue_execution(RescueExecution::RecoveryRequired);
    let mut service = PreviewService::new(platform);
    let response = service.handle(&request(Command::RescueToBase { package: allowed() }));
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert_eq!(service.platform().calls(), vec![Call::Rescue]);
}

#[test]
fn reconciliation_reports_recovery_without_converting_it_to_success_enablement() {
    // Given
    let platform = FakePlatform::default().with_reconcile_outcome(
        uclone_slot_runtime::reconcile::ReconcileOutcome::RecoveryRequired(
            uclone_slot_runtime::reconcile::ReconcileReason::JournalMetadata,
        ),
    );
    let mut service = PreviewService::new(platform);

    // When
    let response = service.handle(&request(Command::ReconcilePackage { package: allowed() }));

    // Then
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::ReconcileReport(report))
            if matches!(
                report.outcome(),
                &ReconcileOutcome::RecoveryRequired {
                    reason: uclone_slot_runtime::reconcile::ReconcileReason::JournalMetadata,
                }
            )
    ));
    assert_eq!(service.platform().calls(), vec![Call::Reconcile]);
}

#[test]
fn every_service_error_has_a_stable_protocol_mapping() {
    let cases = [
        (ServiceError::InvalidRequest, ErrorCode::InvalidRequest),
        (
            ServiceError::PackageNotAllowed,
            ErrorCode::PackageNotAllowed,
        ),
        (ServiceError::NotFound, ErrorCode::NotFound),
        (ServiceError::Conflict, ErrorCode::Conflict),
        (ServiceError::RecoveryRequired, ErrorCode::RecoveryRequired),
        (ServiceError::Busy, ErrorCode::Busy),
        (ServiceError::Quarantined, ErrorCode::Quarantined),
        (ServiceError::UserLocked, ErrorCode::UserLocked),
        (
            ServiceError::UnsupportedDevice,
            ErrorCode::UnsupportedDevice,
        ),
        (ServiceError::Internal, ErrorCode::Internal),
    ];

    for (error, expected) in cases {
        assert_eq!(error.code(), expected);
    }
}
