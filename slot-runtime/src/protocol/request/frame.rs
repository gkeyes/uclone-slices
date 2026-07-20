use serde::Deserialize as _;

use super::{Request, WireRequest, from_wire};
use crate::protocol::{ProtocolError, encode_json_line, frame_line};

/// Encodes one request as a bounded JSON-lines frame.
pub fn encode_request(request: &Request) -> Result<Vec<u8>, ProtocolError> {
    encode_json_line(request)
}

/// Decodes one request, rejecting missing delimiters, trailing bytes, and extra frames.
pub fn decode_request(frame: &[u8]) -> Result<Request, ProtocolError> {
    let line = frame_line(frame)?;
    let mut deserializer = serde_json::Deserializer::from_slice(line);
    let wire = WireRequest::deserialize(&mut deserializer)?;
    deserializer.end().map_err(ProtocolError::Json)?;
    from_wire(wire)
}
