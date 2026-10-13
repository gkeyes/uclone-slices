use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) const OPERATION_FAILED_RESPONSE: &str = "{\"error\":{\"code\":\"operation_failed\"}}\n";
const INVALID_REQUEST_RESPONSE: &str = "{\"error\":{\"code\":\"invalid_request\"}}\n";
const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Time a client has to deliver its whole request line. The Manager writes the line right
/// after connecting; this only frees a thread held by a client that never finishes it.
/// Handling the request and writing the response are not limited.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Connections served at once. Requests already run one at a time behind the transaction
/// lock, so this only bounds idle threads; a connection over the limit is not refused, it
/// waits in the listen backlog until a running one finishes.
pub(super) const MAX_CONCURRENT_CONNECTIONS: usize = 32;

pub(super) type RequestHandler = dyn Fn(&str) -> String + Send + Sync + 'static;

/// Counting gate for concurrently served connections.
#[derive(Debug)]
pub(super) struct ConnectionLimit {
    maximum: usize,
    active: Mutex<usize>,
    released: Condvar,
}

/// One served connection; dropping it frees the slot.
#[derive(Debug)]
pub(super) struct ConnectionPermit {
    limit: Arc<ConnectionLimit>,
}

impl ConnectionLimit {
    pub(super) fn new(maximum: usize) -> Arc<Self> {
        Arc::new(Self {
            maximum: maximum.max(1),
            active: Mutex::new(0),
            released: Condvar::new(),
        })
    }

    /// Waits until fewer than `maximum` connections are being served.
    pub(super) fn acquire(self: &Arc<Self>) -> ConnectionPermit {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        while *active >= self.maximum {
            active = self
                .released
                .wait(active)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *active += 1;
        ConnectionPermit {
            limit: Arc::clone(self),
        }
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        let mut active = self
            .limit
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *active = active.saturating_sub(1);
        self.limit.released.notify_one();
    }
}

pub(super) fn spawn_connection(
    stream: UnixStream,
    handler: Arc<RequestHandler>,
    permit: ConnectionPermit,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let _permit = permit;
        if let Err(error) = serve_one(stream, REQUEST_READ_TIMEOUT, |line| handler(line)) {
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
    read_timeout: Duration,
    handler: impl FnOnce(&str) -> String,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = match read_frame(&stream, read_timeout) {
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

/// Reads one newline-terminated frame. `timeout` bounds the whole line, not each read, so a
/// client trickling bytes cannot hold the connection open either.
fn read_frame(stream: &UnixStream, timeout: Duration) -> Result<String, FrameError> {
    let deadline = Instant::now() + timeout;
    let mut reader = BufReader::new(stream.try_clone().map_err(FrameError::Io)?);
    let mut frame = Vec::with_capacity(1024);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(FrameError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "request line was not received in time",
            )));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(FrameError::Io)?;
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
