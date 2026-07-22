#![allow(
    missing_docs,
    clippy::unwrap_used,
    reason = "validated protocol fixtures fail the invoking test immediately"
)]

use uclone_slot_runtime::domain::{PackageName, PackageSupportLevel, SlotId};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::protocol::{
    ALLOWED_PACKAGE, Ack, AckOperation, Command, ErrorCode, MAX_FRAME_SIZE, PackageStatus,
    ProbeReport, ProtocolError, ReconcileOutcome, ReconcileReport, Request, RequestId, Response,
    ResponsePayload, ResponseStatus, SCHEMA_VERSION, SwitchResult, decode_request, decode_response,
    encode_request, encode_response,
};
use uclone_slot_runtime::reconcile::ReconcileReason;
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotSeedMode};
use uclone_slot_runtime::target::PREVIEW_SLOT;

fn id(value: &str) -> RequestId {
    RequestId::new(value).unwrap()
}

fn package(value: &str) -> PackageName {
    PackageName::parse(value).unwrap()
}

fn allowed() -> PackageName {
    package(ALLOWED_PACKAGE)
}

#[test]
fn all_fixed_commands_round_trip_as_strict_json_lines() {
    let slot = SlotId::parse(PREVIEW_SLOT).unwrap();
    let label = SlotDisplayName::parse("Work profile").unwrap();
    let commands = vec![
        Command::Probe,
        Command::InspectPackage { package: allowed() },
        Command::ListManagedApps,
        Command::EnrollPackage {
            package: allowed(),
            accept_direct_boot_conditional: false,
        },
        Command::StatusPackage { package: allowed() },
        Command::CreateSlot {
            package: allowed(),
            display_name: label.clone(),
            seed_mode: SlotSeedMode::Blank,
        },
        Command::ListSlots { package: allowed() },
        Command::Switch {
            package: allowed(),
            slot: slot.clone(),
        },
        Command::RenameSlot {
            package: allowed(),
            slot: slot.clone(),
            display_name: label,
        },
        Command::DeleteSlot {
            package: allowed(),
            slot,
        },
        Command::Reconcile,
        Command::ReconcilePackage { package: allowed() },
        Command::RetirePackage { package: allowed() },
        Command::RescueToBase { package: allowed() },
    ];

    for (index, command) in commands.into_iter().enumerate() {
        let request = Request::new(id(&format!("request-{index}")), command).unwrap();
        let frame = encode_request(&request).unwrap();
        assert_eq!(frame.last(), Some(&b'\n'));
        assert_eq!(decode_request(&frame).unwrap(), request);
    }
}

#[test]
fn direct_boot_enrollment_confirmation_is_explicit_and_round_trips() {
    let command = Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: true,
    };
    let request = Request::new(id("direct-boot-consent"), command.clone()).unwrap();

    let frame = encode_request(&request).unwrap();

    assert!(
        std::str::from_utf8(&frame)
            .unwrap()
            .contains(r#""accept_direct_boot_conditional":true"#,)
    );
    assert_eq!(decode_request(&frame).unwrap().command(), &command);
    assert_eq!(
        PackageSupportLevel::DirectBootConditional.as_str(),
        "direct_boot_conditional",
    );
}

#[test]
fn legacy_enrollment_request_defaults_direct_boot_confirmation_to_false() {
    let frame = br#"{"schema_version":1,"request_id":"legacy-enroll","command":"enroll_package","package":"com.asksky.fitness"}
"#;

    let decoded = decode_request(frame).unwrap();

    assert!(matches!(
        decoded.command(),
        Command::EnrollPackage {
            accept_direct_boot_conditional: false,
            ..
        }
    ));
}

#[test]
fn wire_shape_rejects_schema_drift_and_unscoped_fields() {
    let valid = br#"{"schema_version":1,"request_id":"r1","command":"probe"}
"#;
    assert!(decode_request(valid).is_ok());

    let unsupported = br#"{"schema_version":2,"request_id":"r1","command":"probe"}
"#;
    assert!(matches!(
        decode_request(unsupported),
        Err(ProtocolError::UnsupportedSchema(2))
    ));

    let path = br#"{"schema_version":1,"request_id":"r1","command":"probe","runtime_root":"/data"}
"#;
    assert!(matches!(decode_request(path), Err(ProtocolError::Json(_))));

    let user = br#"{"schema_version":1,"request_id":"r1","command":"probe","user_id":0}
"#;
    assert!(matches!(decode_request(user), Err(ProtocolError::Json(_))));
}

#[test]
fn any_well_formed_package_is_accepted_without_accepting_paths() {
    let package = package("com.example.other");
    let request = Request::new(id("r1"), Command::StatusPackage { package });
    assert!(request.is_ok());

    let wire = br#"{"schema_version":1,"request_id":"r1","command":"status_package","package":"com.example.other"}
"#;
    assert!(decode_request(wire).is_ok());
    assert!(serde_json::from_slice::<Request>(wire).is_ok());

    let path = br#"{"schema_version":1,"request_id":"r1","command":"status_package","package":"../../data"}
"#;
    assert!(matches!(decode_request(path), Err(ProtocolError::Json(_))));
}

#[test]
fn framing_rejects_missing_trailing_and_multiple_records_and_oversize() {
    let request = Request::new(id("r1"), Command::Probe).unwrap();
    let frame = encode_request(&request).unwrap();
    assert!(matches!(
        decode_request(frame.strip_suffix(b"\n").unwrap()),
        Err(ProtocolError::MissingDelimiter)
    ));

    let mut multiple = frame.clone();
    multiple.extend_from_slice(&frame);
    assert!(matches!(
        decode_request(&multiple),
        Err(ProtocolError::MultipleFrames)
    ));

    let mut trailing = frame.clone();
    trailing.push(b'x');
    assert!(matches!(
        decode_request(&trailing),
        Err(ProtocolError::TrailingBytes)
    ));

    let oversized = vec![b'x'; MAX_FRAME_SIZE + 1];
    assert!(matches!(
        decode_request(&oversized),
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}

#[test]
fn response_round_trip_has_typed_status_and_error_code() {
    let response = Response::error(id("r1"), ErrorCode::RecoveryRequired);
    let frame = encode_response(&response).unwrap();
    let decoded = decode_response(&frame).unwrap();
    assert_eq!(decoded.request_id().as_str(), "r1");
    assert_eq!(decoded.status(), ResponseStatus::Error);
    assert_eq!(decoded.error_code(), Some(ErrorCode::RecoveryRequired));

    let invalid = br#"{"schema_version":1,"request_id":"r1","status":"ok","error_code":"busy"}
"#;
    assert!(matches!(
        decode_response(invalid),
        Err(ProtocolError::InvalidResponse)
    ));
}

#[test]
fn success_payloads_are_typed_bounded_and_round_trip() {
    let payloads = [
        ResponsePayload::ProbeReport(ProbeReport::new(true, true, true)),
        ResponsePayload::PackageStatus(PackageStatus::new(
            allowed(),
            SlotId::base(),
            LifecycleState::Normal,
            true,
            false,
        )),
        ResponsePayload::SwitchResult(SwitchResult::new(
            allowed(),
            SlotId::parse("slot_a").unwrap(),
        )),
        ResponsePayload::ReconcileReport(ReconcileReport::new(
            allowed(),
            ReconcileOutcome::RecoveryRequired {
                reason: ReconcileReason::GateRestoreFailed,
            },
        )),
        ResponsePayload::Ack(Ack::new(AckOperation::EnrollPackage)),
    ];

    for (index, payload) in payloads.into_iter().enumerate() {
        let response = Response::ok(id(&format!("payload-{index}")), payload).unwrap();
        let decoded = decode_response(&encode_response(&response).unwrap()).unwrap();
        assert_eq!(decoded, response);
        assert!(decoded.payload().is_some());
    }
}

#[test]
fn reconciliation_recovery_exposes_a_stable_stage_reason() {
    let payload = ResponsePayload::ReconcileReport(ReconcileReport::new(
        allowed(),
        ReconcileOutcome::RecoveryRequired {
            reason: ReconcileReason::ViewRestoreFailed,
        },
    ));
    let response = Response::ok(id("reconcile-reason"), payload).unwrap();

    let frame = encode_response(&response).unwrap();

    assert!(
        std::str::from_utf8(&frame)
            .unwrap()
            .contains(r#""kind":"recovery_required","reason":"view_restore_failed""#)
    );
    assert_eq!(decode_response(&frame).unwrap(), response);
}

#[test]
fn response_payload_and_status_invariants_fail_closed() {
    let missing_payload = br#"{"schema_version":1,"request_id":"r1","status":"ok"}
"#;
    assert!(matches!(
        decode_response(missing_payload),
        Err(ProtocolError::InvalidResponse)
    ));

    let error_with_payload = br#"{"schema_version":1,"request_id":"r1","status":"error","error_code":"busy","payload":{"kind":"ack","data":{"operation":"enroll_package"}}}
"#;
    assert!(matches!(
        decode_response(error_with_payload),
        Err(ProtocolError::InvalidResponse)
    ));

    let unknown_payload_field = br#"{"schema_version":1,"request_id":"r1","status":"ok","payload":{"kind":"probe_report","data":{"package":"com.uclone.slotprobe","ready":true,"user_unlocked":true,"ce_de_supported":true,"path":"/data"}}}
"#;
    assert!(matches!(
        decode_response(unknown_payload_field),
        Err(ProtocolError::Json(_))
    ));

    let generic = Response::ok(
        id("r1"),
        ResponsePayload::PackageStatus(PackageStatus::new(
            package("com.example.other"),
            SlotId::base(),
            LifecycleState::Normal,
            true,
            false,
        )),
    );
    assert!(generic.is_ok());
}

#[test]
fn constants_are_versioned_and_bounded() {
    assert_eq!(SCHEMA_VERSION, 1);
    assert_eq!(MAX_FRAME_SIZE, 16 * 1024);
}
