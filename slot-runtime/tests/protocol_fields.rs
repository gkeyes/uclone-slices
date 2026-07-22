#![allow(
    missing_docs,
    clippy::unwrap_used,
    reason = "validated protocol fixtures fail the invoking test immediately"
)]

use uclone_slot_runtime::domain::PackageName;
use uclone_slot_runtime::protocol::{
    ALLOWED_PACKAGE, Command, ProtocolError, Request, RequestId, decode_request, encode_request,
};

fn allowed() -> PackageName {
    PackageName::parse(ALLOWED_PACKAGE).unwrap()
}

#[test]
fn direct_boot_enrollment_confirmation_is_explicit_and_round_trips() {
    let command = Command::EnrollPackage {
        package: allowed(),
        accept_direct_boot_conditional: true,
    };
    let request = Request::new(
        RequestId::new("direct-boot-consent").unwrap(),
        command.clone(),
    )
    .unwrap();

    let frame = encode_request(&request).unwrap();

    assert!(
        std::str::from_utf8(&frame)
            .unwrap()
            .contains(r#""accept_direct_boot_conditional":true"#)
    );
    assert_eq!(decode_request(&frame).unwrap().command(), &command);
}

#[test]
fn missing_enrollment_consent_defaults_to_false() {
    let frame = br#"{"schema_version":2,"request_id":"legacy-enroll","command":"enroll_package","build_id":"development","package":"com.asksky.fitness"}
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
fn non_rescue_command_requires_the_exact_paired_build() {
    let missing = br#"{"schema_version":2,"request_id":"r1","command":"status_package","package":"com.example.app"}
"#;
    let mismatch = br#"{"schema_version":2,"request_id":"r2","command":"status_package","build_id":"other","package":"com.example.app"}
"#;
    assert!(matches!(
        decode_request(missing),
        Err(ProtocolError::RuntimePairMismatch)
    ));
    assert!(matches!(
        decode_request(mismatch),
        Err(ProtocolError::RuntimePairMismatch)
    ));
}

#[test]
fn consent_field_is_rejected_outside_enrollment() {
    let frame = br#"{"schema_version":2,"request_id":"r1","command":"status_package","package":"com.example.app","accept_direct_boot_conditional":true}
"#;
    assert!(matches!(
        decode_request(frame),
        Err(ProtocolError::UnexpectedField(
            "accept_direct_boot_conditional"
        ))
    ));
}

#[test]
fn wire_shape_rejects_schema_drift_and_unscoped_fields() {
    let valid = br#"{"schema_version":2,"request_id":"r1","command":"probe"}
"#;
    assert!(decode_request(valid).is_ok());
    let unsupported = br#"{"schema_version":1,"request_id":"r1","command":"probe"}
"#;
    assert!(matches!(
        decode_request(unsupported),
        Err(ProtocolError::UnsupportedSchema(1))
    ));
    let path = br#"{"schema_version":2,"request_id":"r1","command":"probe","runtime_root":"/data"}
"#;
    assert!(matches!(decode_request(path), Err(ProtocolError::Json(_))));
    let user = br#"{"schema_version":2,"request_id":"r1","command":"probe","user_id":0}
"#;
    assert!(matches!(decode_request(user), Err(ProtocolError::Json(_))));
}
