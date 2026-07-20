#![allow(missing_docs)]

mod service_support;

use service_support::{FakePlatform, allowed, preview_view, ready_base, request};
use uclone_slot_runtime::daemon::{MutationGuard, RequestHandler};
use uclone_slot_runtime::protocol::{Command, ErrorCode, ResponseStatus};
use uclone_slot_runtime::service::PreviewService;

#[test]
fn every_mutating_command_returns_busy_before_platform_access() {
    // Given
    let guard = MutationGuard::new();
    let held_guard = guard.clone();
    let permit = held_guard.try_acquire().unwrap();
    let commands = [
        Command::EnrollPackage { package: allowed() },
        Command::Switch {
            package: allowed(),
            slot: preview_view().slot_id().clone(),
        },
        Command::Reconcile,
        Command::RescueToBase { package: allowed() },
    ];

    for command in commands {
        let mut service = PreviewService::with_mutation_guard(
            FakePlatform::with_state(ready_base()),
            guard.clone(),
        );

        // When
        let response = service.handle(&request(command));

        // Then
        assert_eq!(response.error_code(), Some(ErrorCode::Busy));
        assert!(service.platform().calls().is_empty());
    }
    drop(permit);
}

#[test]
fn read_only_commands_remain_available_while_mutation_gate_is_held() {
    // Given
    let guard = MutationGuard::new();
    let held_guard = guard.clone();
    let permit = held_guard.try_acquire().unwrap();
    let mut service =
        PreviewService::with_mutation_guard(FakePlatform::with_state(ready_base()), guard);

    // When
    let probe = service.handle(&request(Command::Probe));
    let status = service.handle(&request(Command::StatusPackage { package: allowed() }));

    // Then
    assert_eq!(probe.status(), ResponseStatus::Ok);
    assert_eq!(status.status(), ResponseStatus::Ok);
    drop(permit);
}
