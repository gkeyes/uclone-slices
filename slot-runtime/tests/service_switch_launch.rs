#![allow(missing_docs)]

mod service_support;

use service_support::{Call, FakePlatform, allowed, preview_view, ready_base, request};
use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::domain::{ManagedPackage, PackageKey};
use uclone_slot_runtime::launch::LaunchDisposition;
use uclone_slot_runtime::protocol::{
    Command, ErrorCode, LaunchStatus, ResponsePayload, ResponseStatus,
};
use uclone_slot_runtime::service::{
    ObservedGateState, PackageSnapshot, PackageState, PreviewService, ServiceError,
};
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotSeedMode};

fn ready_on_preview(enabled: bool, suspended: bool) -> PackageState {
    let PackageState::Ready(base) = ready_base() else {
        return PackageState::RecoveryRequired;
    };
    let original = base.managed();
    let preview = preview_view();
    let managed = ManagedPackage::new(
        PackageKey::new(original.package_name().clone(), original.user_id()),
        original.identity().clone(),
        original.base_inodes(),
        preview.clone(),
        original.lifecycle_state(),
    )
    .unwrap();
    PackageState::Ready(Box::new(PackageSnapshot::new(
        managed,
        vec![preview],
        ObservedGateState::new(enabled, suspended),
    )))
}

fn ready_base_with_preview() -> PackageState {
    let PackageState::Ready(base) = ready_base() else {
        return PackageState::RecoveryRequired;
    };
    PackageState::Ready(Box::new(PackageSnapshot::new(
        base.managed().clone(),
        vec![preview_view()],
        base.gate(),
    )))
}

fn launch_current(
    service: &mut PreviewService<FakePlatform>,
) -> uclone_slot_runtime::protocol::Response {
    service.handle(&request(Command::LaunchCurrent {
        package: allowed(),
        expected_slot: preview_view().slot_id().clone(),
    }))
}

#[test]
fn switch_commits_without_launching_then_explicit_launch_revalidates_same_slot() {
    let mut service = PreviewService::new(FakePlatform::with_state(ready_base_with_preview()));

    let switched = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));

    assert!(matches!(
        switched.payload(),
        Some(ResponsePayload::SwitchResult(result)) if result.slot() == preview_view().slot_id()
    ));
    assert!(!service.platform().calls().contains(&Call::Launch));
    service.platform().clear_calls();

    let launched = launch_current(&mut service);

    assert!(matches!(
        launched.payload(),
        Some(ResponsePayload::LaunchResult(result))
            if result.slot() == preview_view().slot_id()
                && result.launch_status() == LaunchStatus::Launched
    ));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::State,
            Call::VerifyCurrent,
            Call::State,
            Call::Launch
        ]
    );
}

#[test]
fn launch_requires_the_client_confirmed_active_slot() {
    let mut service = PreviewService::new(FakePlatform::with_state(ready_base()));

    let response = launch_current(&mut service);

    assert_eq!(response.error_code(), Some(ErrorCode::Conflict));
    assert_eq!(service.platform().calls(), vec![Call::Probe, Call::State]);
}

#[test]
fn launch_verification_failure_never_starts_the_app() {
    let platform = FakePlatform::with_state(ready_on_preview(true, false)).fail_at(
        service_support::FailurePoint::VerifyCurrent,
        ServiceError::RecoveryRequired,
    );
    let mut service = PreviewService::new(platform);

    let response = launch_current(&mut service);

    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert!(!service.platform().calls().contains(&Call::Launch));
}

#[test]
fn launch_preserves_original_disabled_or_suspended_gate() {
    for state in [ready_on_preview(false, false), ready_on_preview(true, true)] {
        let mut service = PreviewService::new(FakePlatform::with_state(state));

        let response = launch_current(&mut service);

        assert!(matches!(
            response.payload(),
            Some(ResponsePayload::LaunchResult(result))
                if result.launch_status() == LaunchStatus::GateBlocked
        ));
        assert!(service.platform().calls().contains(&Call::VerifyCurrent));
        assert!(!service.platform().calls().contains(&Call::Launch));
    }
}

#[test]
fn identity_drift_after_proof_quarantines_without_claiming_success() {
    let platform = FakePlatform::with_state(ready_on_preview(true, false))
        .with_launch_disposition(LaunchDisposition::IdentityChanged);
    let mut service = PreviewService::new(platform);

    let response = launch_current(&mut service);

    assert_eq!(response.error_code(), Some(ErrorCode::Quarantined));
    assert!(
        service
            .platform()
            .calls()
            .ends_with(&[Call::Launch, Call::Contain(ServiceError::Quarantined),])
    );
}

#[test]
fn package_state_drift_after_proof_is_contained_as_recovery_required() {
    let platform = FakePlatform::with_state(ready_on_preview(true, false))
        .with_launch_disposition(LaunchDisposition::PackageStateChanged);
    let mut service = PreviewService::new(platform);

    let response = launch_current(&mut service);

    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert!(
        service
            .platform()
            .calls()
            .ends_with(&[Call::Launch, Call::Contain(ServiceError::RecoveryRequired),])
    );
}

#[test]
fn create_returns_a_committed_switch_result_without_launching() {
    let mut service = PreviewService::new(FakePlatform::with_state(ready_base()));

    let response = service.handle(&request(Command::CreateSlot {
        package: allowed(),
        display_name: SlotDisplayName::parse("Fresh space").unwrap(),
        seed_mode: SlotSeedMode::Blank,
    }));

    assert_eq!(response.status(), ResponseStatus::Ok);
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::SwitchResult(result)) if result.slot() == preview_view().slot_id()
    ));
    assert!(!service.platform().calls().contains(&Call::Launch));
}
