#![doc = "Restricted slotctl parser, transport, and surface tests."]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test fixtures should fail the individual test immediately"
)]

use std::process::Command as ProcessCommand;

use clap::{CommandFactory, Parser};
use uclone_slot_runtime::cli::{Cli, CliCommand, Transport, execute, execute_request};
use uclone_slot_runtime::domain::PackageName;
use uclone_slot_runtime::protocol::{
    ALLOWED_PACKAGE, Ack, AckOperation, Command, ErrorCode, ProtocolError, Request, Response,
    ResponsePayload, ResponseStatus,
};

#[derive(Debug)]
struct FakeTransport {
    response: Option<Response>,
    requests: Vec<Request>,
}

impl FakeTransport {
    const fn response(response: Response) -> Self {
        Self {
            response: Some(response),
            requests: Vec::new(),
        }
    }
}

impl Transport for FakeTransport {
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        self.requests.push(request.clone());
        self.response
            .take()
            .ok_or_else(|| ProtocolError::Io(std::io::Error::other("missing fake response")))
    }
}

fn success_response(request: &Request) -> Response {
    Response::ok(
        request.request_id().clone(),
        ResponsePayload::Ack(Ack::new(AckOperation::EnrollPackage)),
    )
    .unwrap()
}

#[test]
fn parser_exposes_only_the_fixed_command_surface() {
    let cases = [
        vec!["slotctl", "probe"],
        vec!["slotctl", "enroll", ALLOWED_PACKAGE],
        vec!["slotctl", "status", ALLOWED_PACKAGE],
        vec!["slotctl", "switch", ALLOWED_PACKAGE, "work"],
        vec!["slotctl", "reconcile"],
        vec!["slotctl", "rescue", ALLOWED_PACKAGE, "--to-base"],
    ];
    for args in cases {
        assert!(Cli::try_parse_from(args).is_ok());
    }
    let help = Cli::command().render_help().to_string();
    assert!(help.contains("probe"));
    assert!(help.contains("rescue"));
    assert!(!help.contains("runtime-root"));
    assert!(!help.contains("shell"));
    assert!(!help.contains("user-id"));
    assert!(!help.contains("path"));
}

#[test]
fn rescue_requires_the_explicit_base_flag() {
    let error = Cli::try_parse_from(["slotctl", "rescue", ALLOWED_PACKAGE]);
    assert!(error.is_err());
}

#[test]
fn request_conversion_uses_protocol_allowlist_and_bounded_ids() {
    let command = CliCommand::Status {
        package: ALLOWED_PACKAGE.to_owned(),
    };
    let request = command.request().unwrap();
    assert!(request.request_id().as_str().len() <= 128);
    assert!(request.request_id().as_str().starts_with("slotctl-"));
    assert!(matches!(request.command(), Command::StatusPackage { .. }));

    let rejected = CliCommand::Status {
        package: "com.example.other".to_owned(),
    }
    .request();
    assert!(matches!(
        rejected,
        Err(uclone_slot_runtime::cli::CliError::Protocol(
            ProtocolError::PackageNotAllowed(_)
        ))
    ));
}
#[test]
fn injected_transport_prints_one_json_response_and_reports_daemon_error() {
    let command = CliCommand::Probe;
    let request = command.request().unwrap();
    let mut transport = FakeTransport::response(Response::error(
        request.request_id().clone(),
        ErrorCode::Busy,
    ));
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let result = execute_request(&request, &mut transport, &mut output, &mut diagnostics);
    assert!(result.is_err());
    assert!(
        output
            .strip_suffix(b"\n")
            .is_some_and(|body| !body.contains(&b'\n'))
    );
    let decoded = uclone_slot_runtime::protocol::decode_response(&output).unwrap();
    assert_eq!(decoded.status(), ResponseStatus::Error);
    assert_eq!(decoded.error_code(), Some(ErrorCode::Busy));
    assert!(!diagnostics.is_empty());
}
#[test]
fn injected_transport_preserves_the_request_and_success_output() {
    let command = CliCommand::Enroll {
        package: ALLOWED_PACKAGE.to_owned(),
    };
    let request = command.request().unwrap();
    let mut transport = FakeTransport::response(success_response(&request));
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    execute_request(&request, &mut transport, &mut output, &mut diagnostics).unwrap();
    assert!(diagnostics.is_empty());
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(
        transport.requests.first().map(Request::command),
        Some(request.command())
    );
    assert!(output.ends_with(b"\n"));
}
#[test]
fn injected_transport_rejects_a_response_for_another_request_without_output() {
    let request = CliCommand::Probe.request().unwrap();
    let other_request = CliCommand::Reconcile.request().unwrap();
    let mut transport = FakeTransport::response(Response::error(
        other_request.request_id().clone(),
        ErrorCode::Busy,
    ));
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let result = execute_request(&request, &mut transport, &mut output, &mut diagnostics);
    assert!(matches!(
        result,
        Err(uclone_slot_runtime::cli::CliError::Protocol(
            ProtocolError::RequestIdMismatch { .. }
        ))
    ));
    assert!(output.is_empty());
    assert!(!diagnostics.is_empty());
}
#[test]
fn execute_request_reuses_the_prevalidated_request_id() {
    let request = CliCommand::Probe.request().unwrap();
    let mut transport = FakeTransport::response(success_response(&request));
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    execute_request(&request, &mut transport, &mut output, &mut diagnostics).unwrap();
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(
        transport.requests.first().map(Request::request_id),
        Some(request.request_id())
    );
}
#[test]
fn binary_help_and_invalid_command_are_manual_surface_checks() {
    let binary = env!("CARGO_BIN_EXE_slotctl");
    let help = ProcessCommand::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: slotctl"));

    let invalid = ProcessCommand::new(binary)
        .args(["unknown"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!invalid.stderr.is_empty());
}

#[test]
fn binary_rejects_well_formed_non_allowlisted_package_before_connecting() {
    let binary = env!("CARGO_BIN_EXE_slotctl");
    let rejected = ProcessCommand::new(binary)
        .args(["status", "com.asksky.fitness"])
        .output()
        .unwrap();

    assert!(!rejected.status.success());
    let diagnostics = String::from_utf8_lossy(&rejected.stderr);
    assert!(diagnostics.contains("not allowlisted"));
    assert!(!diagnostics.contains("No such file or directory"));
}

#[test]
fn package_parser_rejects_malformed_names_before_transport() {
    let command = CliCommand::Status {
        package: "not a package".to_owned(),
    };
    let mut transport = FakeTransport::response(Response::error(
        uclone_slot_runtime::protocol::RequestId::new("unused").unwrap(),
        ErrorCode::Internal,
    ));
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let result = execute(&command, &mut transport, &mut output, &mut diagnostics);
    assert!(matches!(
        result,
        Err(uclone_slot_runtime::cli::CliError::InvalidArgument(_))
    ));
    assert!(transport.requests.is_empty());
    assert!(output.is_empty());
    assert!(!diagnostics.is_empty());
    assert_eq!(
        PackageName::parse("com.uclone.slotprobe").unwrap().as_str(),
        ALLOWED_PACKAGE
    );
}
