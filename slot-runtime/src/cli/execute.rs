use std::io::Write;

use crate::protocol::{ProtocolError, Request, Response, ResponseStatus, encode_response};

use super::{CliCommand, CliError, Transport, fail};

/// Builds one request from a parsed command and executes it once.
pub fn execute<T, O, E>(
    command: &CliCommand,
    transport: &mut T,
    output: &mut O,
    diagnostics: &mut E,
) -> Result<(), CliError>
where
    T: Transport,
    O: Write,
    E: Write,
{
    let request = match command.request() {
        Ok(request) => request,
        Err(error) => return fail(error, diagnostics),
    };
    execute_request(&request, transport, output, diagnostics)
}

/// Executes an already validated request without rebuilding its request id.
pub fn execute_request<T, O, E>(
    request: &Request,
    transport: &mut T,
    output: &mut O,
    diagnostics: &mut E,
) -> Result<(), CliError>
where
    T: Transport,
    O: Write,
    E: Write,
{
    let response = match transport.request(request) {
        Ok(response) => response,
        Err(error) => return fail(CliError::Protocol(error), diagnostics),
    };
    write_response(request, &response, output, diagnostics)
}

pub(super) fn write_response<O, E>(
    request: &Request,
    response: &Response,
    output: &mut O,
    diagnostics: &mut E,
) -> Result<(), CliError>
where
    O: Write,
    E: Write,
{
    if response.request_id() != request.request_id() {
        return fail(
            CliError::Protocol(ProtocolError::RequestIdMismatch {
                expected: request.request_id().clone(),
                actual: response.request_id().clone(),
            }),
            diagnostics,
        );
    }
    let frame = match encode_response(response) {
        Ok(frame) => frame,
        Err(error) => return fail(CliError::Protocol(error), diagnostics),
    };
    if let Err(error) = output.write_all(&frame).and_then(|()| output.flush()) {
        return fail(CliError::Io(error), diagnostics);
    }
    if response.status() == ResponseStatus::Error {
        let error = response.error_code().map_or(
            CliError::Protocol(ProtocolError::InvalidResponse),
            CliError::Daemon,
        );
        return fail(error, diagnostics);
    }
    Ok(())
}
