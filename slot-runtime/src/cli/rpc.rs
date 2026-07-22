use std::io::{BufRead, Write};

use crate::protocol::{Command, MAX_FRAME_SIZE, ProtocolError, decode_request};

use super::{CliError, UnixTransport, execute_request, fail, timeout};

pub(super) fn run<R, O, E>(input: R, output: &mut O, diagnostics: &mut E) -> Result<(), CliError>
where
    R: BufRead,
    O: Write,
    E: Write,
{
    let mut frame = Vec::new();
    input
        .take((MAX_FRAME_SIZE + 1) as u64)
        .read_until(b'\n', &mut frame)?;
    if frame.len() > MAX_FRAME_SIZE {
        return fail(
            CliError::Protocol(ProtocolError::FrameTooLarge { size: frame.len() }),
            diagnostics,
        );
    }
    let request = decode_request(&frame).map_err(CliError::Protocol)?;
    let mut transport = match UnixTransport::connect() {
        Ok(transport) => transport,
        Err(_) if matches!(request.command(), Command::RescueToBase { .. }) => {
            return super::direct::run(&request, output, diagnostics);
        }
        Err(error) => return fail(CliError::Protocol(error), diagnostics),
    };
    transport
        .set_timeout(timeout::for_request(request.command()))
        .map_err(CliError::Protocol)?;
    let mut transport =
        super::fallback::RescueFallback::new(&mut transport, super::direct::direct_response);
    execute_request(&request, &mut transport, output, diagnostics)
}
