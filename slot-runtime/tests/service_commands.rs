#![allow(missing_docs)]

mod service_support;

use service_support::{Call, FakePlatform, allowed, preview_view, ready_base, request};
use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::domain::SlotId;
use uclone_slot_runtime::protocol::{
    AckOperation, Command, ReconcileOutcome, ResponsePayload, ResponseStatus,
};
use uclone_slot_runtime::service::{CapabilitySnapshot, PreviewService};
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotSeedMode};

#[test]
fn general_management_commands_return_typed_payloads() {
    let mut service = PreviewService::new(FakePlatform::with_state(ready_base()));

    let inspected = service.handle(&request(Command::InspectPackage { package: allowed() }));
    assert!(matches!(
        inspected.payload(),
        Some(ResponsePayload::PackageInspection(report)) if report.compatible()
    ));

    let apps = service.handle(&request(Command::ListManagedApps));
    assert!(matches!(
        apps.payload(),
        Some(ResponsePayload::ManagedApps(report)) if report.apps().len() == 1
    ));

    let slots = service.handle(&request(Command::ListSlots { package: allowed() }));
    assert!(matches!(
        slots.payload(),
        Some(ResponsePayload::Slots(report)) if report.slots().len() == 1
    ));

    let created = service.handle(&request(Command::CreateSlot {
        package: allowed(),
        display_name: SlotDisplayName::parse("Fresh space").unwrap(),
        seed_mode: SlotSeedMode::Blank,
    }));
    assert!(matches!(
        created.payload(),
        Some(ResponsePayload::SwitchResult(result)) if result.slot() == preview_view().slot_id()
    ));

    for (command, expected) in [
        (
            Command::RenameSlot {
                package: allowed(),
                slot: SlotId::parse("preview").unwrap(),
                display_name: SlotDisplayName::parse("Personal").unwrap(),
            },
            AckOperation::RenameSlot,
        ),
        (
            Command::DeleteSlot {
                package: allowed(),
                slot: SlotId::parse("preview").unwrap(),
            },
            AckOperation::DeleteSlot,
        ),
    ] {
        let response = service.handle(&request(command));
        assert!(matches!(
            response.payload(),
            Some(ResponsePayload::Ack(ack)) if ack.operation() == expected
        ));
    }
}

#[test]
fn direct_boot_enrollment_requires_explicit_confirmation() {
    let platform = FakePlatform::default().with_inspection_compatibility(
        uclone_slot_runtime::domain::PackageCompatibility::new(false, false, true),
    );
    let mut service = PreviewService::new(platform);

    let rejected = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));
    assert_eq!(
        rejected.error_code(),
        Some(uclone_slot_runtime::protocol::ErrorCode::DirectBootConfirmationRequired),
    );
    assert_eq!(
        service.platform().calls(),
        vec![Call::Probe, Call::State, Call::Inspect]
    );

    service.platform().clear_calls();
    let accepted = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: true,
    }));
    assert_eq!(accepted.status(), ResponseStatus::Ok);
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::State,
            Call::Inspect,
            Call::BeginEnrollment,
            Call::HoldGate,
            Call::Quiesce,
            Call::Enroll(true),
            Call::ProveBase,
            Call::RestoreGate,
            Call::RetireGate,
        ],
    );
}

#[test]
fn reconcile_all_retries_instead_of_acknowledging_a_locked_user() {
    let platform = FakePlatform::with_state(ready_base())
        .with_reconcile_outcome(uclone_slot_runtime::reconcile::ReconcileOutcome::Locked);
    let mut service = PreviewService::new(platform);

    let response = service.handle(&request(Command::Reconcile));

    assert_eq!(response.status(), ResponseStatus::Error);
    assert_eq!(
        response.error_code(),
        Some(uclone_slot_runtime::protocol::ErrorCode::UserLocked),
    );
    assert_eq!(
        service.platform().calls(),
        vec![Call::ListManaged, Call::Reconcile]
    );
}

#[test]
fn locked_user_rejects_ce_de_mutation_before_any_package_state_access() {
    let platform = FakePlatform::with_state(ready_base())
        .with_capability(CapabilitySnapshot::new(true, false, true));
    let mut service = PreviewService::new(platform);

    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));

    assert_eq!(
        response.error_code(),
        Some(uclone_slot_runtime::protocol::ErrorCode::UserLocked)
    );
    assert_eq!(service.platform().calls(), vec![Call::Probe]);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one sequential protocol matrix proves command ordering on a single service instance"
)]
fn all_commands_return_typed_payloads_when_platform_proofs_succeed() {
    // Given
    let platform = FakePlatform::default().with_reconcile_outcome(
        uclone_slot_runtime::reconcile::ReconcileOutcome::RestoredSlot(
            preview_view().slot_id().clone(),
        ),
    );
    let mut service = PreviewService::new(platform);

    // When / Then: probe
    let response = service.handle(&request(Command::Probe));
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::ProbeReport(report)) if report.ready()
    ));

    // When / Then: enroll
    let response = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::Ack(ack)) if ack.operation() == AckOperation::EnrollPackage
    ));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::Probe,
            Call::State,
            Call::Inspect,
            Call::BeginEnrollment,
            Call::HoldGate,
            Call::Quiesce,
            Call::Enroll(false),
            Call::ProveBase,
            Call::RestoreGate,
            Call::RetireGate,
        ]
    );
    assert!(!service.platform().enrollment_anchor());

    // When / Then: status is read-only
    service.platform().clear_calls();
    let response = service.handle(&request(Command::StatusPackage { package: allowed() }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::PackageStatus(status)) if status.slot().is_base()
    ));
    assert_eq!(service.platform().calls(), vec![Call::State]);

    // When / Then: first switch materializes before the runtime coordinator
    service.platform().clear_calls();
    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::SwitchResult(result)) if result.slot() == preview_view().slot_id()
    ));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::State,
            Call::CaptureGate,
            Call::HoldGate,
            Call::Quiesce,
            Call::Materialize,
            Call::Switch {
                slot: preview_view().slot_id().clone(),
                prepared: true,
            },
        ]
    );

    // When / Then: ordinary preview-to-base switch uses the runtime coordinator
    service.platform().clear_calls();
    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: uclone_slot_runtime::domain::SlotId::base(),
    }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::SwitchResult(result)) if result.slot().is_base()
    ));
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::State,
            Call::Switch {
                slot: uclone_slot_runtime::domain::SlotId::base(),
                prepared: false,
            },
        ]
    );

    // Given: return to Preview without rematerializing before later commands
    service.platform().clear_calls();
    let response = service.handle(&request(Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    }));
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert_eq!(
        service.platform().calls(),
        vec![
            Call::Probe,
            Call::State,
            Call::Switch {
                slot: preview_view().slot_id().clone(),
                prepared: false,
            },
        ]
    );

    // When / Then: two-phase reconciliation is delegated
    service.platform().clear_calls();
    let response = service.handle(&request(Command::ReconcilePackage { package: allowed() }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::ReconcileReport(report))
            if report.outcome() == &ReconcileOutcome::RestoredSlot {
                slot: preview_view().slot_id().clone(),
            }
    ));
    assert_eq!(service.platform().calls(), vec![Call::Reconcile]);

    // When / Then: rescue derives immutable base internally
    service.platform().clear_calls();
    let response = service.handle(&request(Command::RescueToBase { package: allowed() }));
    assert!(matches!(
        response.payload(),
        Some(ResponsePayload::Ack(ack)) if ack.operation() == AckOperation::RescueToBase
    ));
    assert_eq!(service.platform().calls(), vec![Call::Rescue]);
}

#[test]
fn repeated_mutations_are_idempotent_when_target_is_already_durable() {
    // Given
    let mut service = PreviewService::new(FakePlatform::with_state(ready_base()));

    // When / Then: enrollment is already complete
    let response = service.handle(&request(Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: false,
    }));
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert_eq!(service.platform().calls(), vec![Call::Probe, Call::State]);

    // When: switch once, then repeat
    service.platform().clear_calls();
    let switch = Command::Switch {
        package: allowed(),
        slot: preview_view().slot_id().clone(),
    };
    assert_eq!(
        service.handle(&request(switch.clone())).status(),
        ResponseStatus::Ok
    );
    service.platform().clear_calls();
    let response = service.handle(&request(switch));

    // Then
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert_eq!(service.platform().calls(), vec![Call::Probe, Call::State]);

    // When: rescue once, then repeat
    assert_eq!(
        service
            .handle(&request(Command::RescueToBase { package: allowed() }))
            .status(),
        ResponseStatus::Ok
    );
    service.platform().clear_calls();
    let response = service.handle(&request(Command::RescueToBase { package: allowed() }));

    // Then
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert_eq!(service.platform().calls(), vec![Call::Rescue]);
}
