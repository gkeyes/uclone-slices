use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

use serde::Deserialize;
use serde_json::Value;

use crate::protocol::{ErrorCode, MAX_FRAME_SIZE, RequestId, Response, encode_response};

use super::DaemonError;

pub(super) fn read_frame(stream: &mut UnixStream) -> Result<Option<Vec<u8>>, DaemonError> {
    let mut frame = Vec::with_capacity(256);
    loop {
        let mut byte = [0_u8; 1];
        let count = stream.read(&mut byte)?;
        if count == 0 {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Ok(Some(frame))
            };
        }
        let value = byte
            .first()
            .copied()
            .ok_or_else(|| io::Error::other("UnixStream returned an empty byte read"))?;
        frame.push(value);
        if value == b'\n' {
            return Ok(Some(frame));
        }
        if frame.len() > MAX_FRAME_SIZE {
            drain_oversized_frame(stream)?;
            return Ok(Some(frame));
        }
    }
}

fn drain_oversized_frame(stream: &mut UnixStream) -> Result<(), DaemonError> {
    loop {
        let mut byte = [0_u8; 1];
        let count = stream.read(&mut byte)?;
        if count == 0 || byte.first().copied() == Some(b'\n') {
            return Ok(());
        }
    }
}

pub(super) fn invalid_request_response(frame: &[u8]) -> Option<Response> {
    let line = frame.split(|byte| *byte == b'\n').next()?;
    let request_id = serde_json::from_slice::<Value>(line)
        .ok()
        .and_then(|value| value.get("request_id")?.as_str().map(str::to_owned))
        .or_else(|| truncated_request_id(line))?;
    RequestId::new(&request_id)
        .map(|id| Response::error(id, ErrorCode::InvalidRequest))
        .ok()
}

fn truncated_request_id(line: &[u8]) -> Option<String> {
    let marker = br#""request_id""#;
    let start = line
        .windows(marker.len())
        .position(|window| window == marker)?
        .checked_add(marker.len())?;
    let remainder = line.get(start..)?;
    let colon = remainder.iter().position(|byte| *byte == b':')?;
    let value = remainder.get(colon.checked_add(1)?..)?;
    let value = value.get(value.iter().position(|byte| !byte.is_ascii_whitespace())?..)?;
    let mut deserializer = serde_json::Deserializer::from_slice(value);
    String::deserialize(&mut deserializer).ok()
}

pub(super) fn write_response(
    stream: &mut UnixStream,
    response: &Response,
) -> Result<(), DaemonError> {
    let frame = match encode_response(response) {
        Ok(frame) => frame,
        Err(_) => encode_response(&Response::error(
            response.request_id().clone(),
            ErrorCode::Internal,
        ))?,
    };
    stream.write_all(&frame)?;
    Ok(())
}
