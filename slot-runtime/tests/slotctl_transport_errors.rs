#![doc = "Transport ambiguity tests for the restricted slotctl client."]
#![allow(
    clippy::unwrap_used,
    reason = "test fixtures should fail the individual test immediately"
)]

use uclone_slot_runtime::cli::{CliCommand, Transport, execute_request};
use uclone_slot_runtime::protocol::{ProtocolError, Request, Response};

const SAMPLE_PACKAGE: &str = "com.example.preview";

#[derive(Debug)]
struct FailedTransport {
    error_kind: std::io::ErrorKind,
    requests: Vec<Request>,
}

impl FailedTransport {
    const fn new(error_kind: std::io::ErrorKind) -> Self {
        Self {
            error_kind,
            requests: Vec::new(),
        }
    }
}

impl Transport for FailedTransport {
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        self.requests.push(request.clone());
        Err(std::io::Error::from(self.error_kind).into())
    }
}

#[test]
fn each_connected_transport_failure_is_reported_once_without_a_fabricated_response() {
    let command = CliCommand::Rescue {
        package: SAMPLE_PACKAGE.to_owned(),
        to_base: true,
    };
    for error_kind in [
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::UnexpectedEof,
    ] {
        let request = command.request().unwrap();
        let mut transport = FailedTransport::new(error_kind);
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();

        let result = execute_request(&request, &mut transport, &mut output, &mut diagnostics);

        assert!(matches!(
            result,
            Err(uclone_slot_runtime::cli::CliError::Protocol(
                ProtocolError::Io(ref error)
            )) if error.kind() == error_kind
        ));
        assert_eq!(transport.requests.len(), 1);
        assert!(output.is_empty());
        let diagnostics = String::from_utf8(diagnostics).unwrap();
        assert!(diagnostics.contains("protocol error"));
        assert!(!diagnostics.contains("Busy"));
    }
}
