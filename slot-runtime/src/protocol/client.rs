use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use super::{MAX_FRAME_SIZE, ProtocolError, Request, Response, decode_response, encode_request};

/// Default finite deadline for an ordinary daemon exchange.
pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_mins(30);

/// Small synchronous Unix-domain client for one-request/one-response exchanges.
#[derive(Debug)]
pub struct UnixClient {
    stream: UnixStream,
}

impl UnixClient {
    /// Connects to a caller-supplied Unix socket path.
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, ProtocolError> {
        let mut client = Self {
            stream: UnixStream::connect(path)?,
        };
        client.set_timeout(DEFAULT_CLIENT_TIMEOUT)?;
        Ok(client)
    }

    /// Wraps an already-connected stream, primarily for tests and embedding.
    pub const fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Applies one non-zero read and write deadline to the connected stream.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), ProtocolError> {
        if timeout.is_zero() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "zero client timeout").into());
        }
        self.stream.set_read_timeout(Some(timeout))?;
        self.stream.set_write_timeout(Some(timeout))?;
        Ok(())
    }

    /// Sends one request and reads its matching response.
    pub fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        write_request(&mut self.stream, request)?;
        let response = read_response(&mut self.stream)?;
        if response.request_id() != request.request_id() {
            return Err(ProtocolError::RequestIdMismatch {
                expected: request.request_id().clone(),
                actual: response.request_id().clone(),
            });
        }
        Ok(response)
    }

    /// Returns the wrapped stream.
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }
}

/// Writes one request frame to a connected Unix stream.
pub fn write_request(stream: &mut UnixStream, request: &Request) -> Result<(), ProtocolError> {
    stream.write_all(&encode_request(request)?)?;
    Ok(())
}

/// Reads and decodes exactly one response frame from a connected Unix stream.
pub fn read_response(stream: &mut UnixStream) -> Result<Response, ProtocolError> {
    decode_response(&read_frame(stream)?)
}

fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = Vec::with_capacity(256);
    loop {
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte)? {
            0 => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
            1 => {
                frame.push(byte[0]);
                if frame.len() > MAX_FRAME_SIZE {
                    return Err(ProtocolError::FrameTooLarge { size: frame.len() });
                }
                if byte[0] == b'\n' {
                    return Ok(frame);
                }
            }
            _ => return Err(io::Error::other("UnixStream returned an invalid read length").into()),
        }
    }
}
