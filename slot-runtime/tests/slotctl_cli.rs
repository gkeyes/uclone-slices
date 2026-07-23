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
    Ack, AckOperation, Command, ErrorCode, ProtocolError, Request, Response, ResponsePayload,
    ResponseStatus,
};

const SAMPLE_PACKAGE: &str = "com.example.preview";

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
fn parser_exposes_only_the_typed_command_surface() {
    let cases = [
        vec!["slotctl", "rpc"],
        vec!["slotctl", "emergency-status"],
        vec![
            "slotctl",
            "emergency-status",
            "--cursor",
            "1",
            "--limit",
            "16",
        ],
        vec!["slotctl", "probe"],
        vec!["slotctl", "inspect", SAMPLE_PACKAGE],
        vec!["slotctl", "apps"],
        vec!["slotctl", "enroll", SAMPLE_PACKAGE],
        vec!["slotctl", "status", SAMPLE_PACKAGE],
        vec![
            "slotctl",
            "create",
            SAMPLE_PACKAGE,
            "--name",
            "Work",
            "--seed",
            "blank",
        ],
        vec!["slotctl", "slots", SAMPLE_PACKAGE],
        vec!["slotctl", "switch", SAMPLE_PACKAGE, "work"],
        vec!["slotctl", "launch-current", SAMPLE_PACKAGE, "work"],
        vec![
            "slotctl",
            "rename",
            SAMPLE_PACKAGE,
            "work",
            "--name",
            "Personal",
        ],
        vec!["slotctl", "delete", SAMPLE_PACKAGE, "work"],
        vec!["slotctl", "reconcile"],
        vec!["slotctl", "reconcile", SAMPLE_PACKAGE],
        vec!["slotctl", "retire", SAMPLE_PACKAGE],
        vec!["slotctl", "rescue", SAMPLE_PACKAGE, "--to-base"],
    ];
    for args in cases {
        assert!(Cli::try_parse_from(args).is_ok());
    }
    let help = Cli::command().render_help().to_string();
    assert!(help.contains("probe"));
    assert!(help.contains("emergency-status"));
    assert!(help.contains("rescue"));
    assert!(!help.contains("runtime-root"));
    assert!(!help.contains("shell"));
    assert!(!help.contains("user-id"));
    assert!(!help.contains("path"));
}

#[test]
fn rpc_has_no_shell_or_path_arguments_and_is_not_locally_synthesized() {
    assert!(Cli::try_parse_from(["slotctl", "rpc", "/data"]).is_err());
    let rpc = Cli::try_parse_from(["slotctl", "rpc"]).unwrap();
    assert!(rpc.command.request().is_err());
}

#[test]
fn emergency_status_is_direct_bounded_and_never_builds_a_daemon_request() {
    let status = Cli::try_parse_from([
        "slotctl",
        "emergency-status",
        "--cursor",
        "512",
        "--limit",
        "32",
    ])
    .unwrap();
    assert!(status.command.request().is_err());
    assert!(Cli::try_parse_from(["slotctl", "emergency-status", "--cursor", "513"]).is_err());
    assert!(Cli::try_parse_from(["slotctl", "emergency-status", "--limit", "0"]).is_err());
    assert!(Cli::try_parse_from(["slotctl", "emergency-status", "--limit", "33"]).is_err());
    assert!(Cli::try_parse_from(["slotctl", "emergency-status", "/data"]).is_err());
}

#[test]
fn rescue_requires_the_explicit_base_flag() {
    let error = Cli::try_parse_from(["slotctl", "rescue", SAMPLE_PACKAGE]);
    assert!(error.is_err());
}

#[test]
fn request_conversion_accepts_valid_packages_and_uses_bounded_ids() {
    let command = CliCommand::Status {
        package: SAMPLE_PACKAGE.to_owned(),
    };
    let request = command.request().unwrap();
    assert!(request.request_id().as_str().len() <= 128);
    assert!(request.request_id().as_str().starts_with("slotctl-"));
    assert!(matches!(request.command(), Command::StatusPackage { .. }));

    let second = CliCommand::Status {
        package: "org.example.other".to_owned(),
    }
    .request()
    .unwrap();
    assert!(matches!(second.command(), Command::StatusPackage { .. }));
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
        package: SAMPLE_PACKAGE.to_owned(),
        accept_direct_boot_conditional: false,
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
    let other_request = CliCommand::Reconcile { package: None }.request().unwrap();
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
        PackageName::parse(SAMPLE_PACKAGE).unwrap().as_str(),
        SAMPLE_PACKAGE
    );
}
