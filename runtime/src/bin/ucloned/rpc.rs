use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub(super) const OPERATION_FAILED_RESPONSE: &str = "{\"error\":{\"code\":\"operation_failed\"}}\n";
const INVALID_REQUEST_RESPONSE: &str = "{\"error\":{\"code\":\"invalid_request\"}}\n";
const MAX_FRAME_BYTES: usize = 64 * 1024;

pub(super) type RequestHandler = dyn Fn(&str) -> String + Send + Sync + 'static;

pub(super) fn spawn_connection(stream: UnixStream, handler: Arc<RequestHandler>) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(error) = serve_one(stream, |line| handler(line)) {
            eprintln!("ucloned request failed: {error}");
        }
    })
}

pub(super) fn serialize_transaction(
    lock: &Mutex<()>,
    transaction: impl FnOnce() -> String,
) -> String {
    let _transaction = match lock.lock() {
        Ok(transaction) => transaction,
        Err(error) => {
            eprintln!("ucloned transaction lock failed: {error}");
            return OPERATION_FAILED_RESPONSE.to_owned();
        }
    };
    transaction()
}

fn serve_one(
    mut stream: UnixStream,
    handler: impl FnOnce(&str) -> String,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = match read_frame(&stream) {
        Ok(frame) => frame,
        Err(FrameError::InvalidRequest) => {
            stream.write_all(INVALID_REQUEST_RESPONSE.as_bytes())?;
            return Ok(());
        }
        Err(FrameError::Io(error)) => return Err(Box::new(error)),
    };
    let response = handler(&frame);
    stream.write_all(response.as_bytes())?;
    Ok(())
}

#[derive(Debug)]
enum FrameError {
    InvalidRequest,
    Io(std::io::Error),
}

fn read_frame(stream: &UnixStream) -> Result<String, FrameError> {
    let mut reader = BufReader::new(stream.try_clone().map_err(FrameError::Io)?);
    let mut frame = Vec::with_capacity(1024);
    loop {
        let available = reader.fill_buf().map_err(FrameError::Io)?;
        if available.is_empty() {
            return Err(FrameError::InvalidRequest);
        }
        if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
            if frame.len() + newline > MAX_FRAME_BYTES {
                return Err(FrameError::InvalidRequest);
            }
            frame.extend_from_slice(&available[..newline]);
            reader.consume(newline + 1);
            return String::from_utf8(frame).map_err(|_error| FrameError::InvalidRequest);
        }
        if frame.len() + available.len() > MAX_FRAME_BYTES {
            return Err(FrameError::InvalidRequest);
        }
        let consumed = available.len();
        frame.extend_from_slice(available);
        reader.consume(consumed);
    }
}

#[cfg(test)]
#[path = "rpc_tests.rs"]
mod tests;
