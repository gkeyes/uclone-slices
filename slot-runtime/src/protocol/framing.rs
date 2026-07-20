use serde::Serialize;

use super::{MAX_FRAME_SIZE, ProtocolError};

pub(super) fn encode_json_line<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = serde_json::to_vec(value)?;
    frame.push(b'\n');
    if frame.len() > MAX_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge { size: frame.len() });
    }
    Ok(frame)
}

pub(super) fn frame_line(frame: &[u8]) -> Result<&[u8], ProtocolError> {
    if frame.len() > MAX_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge { size: frame.len() });
    }
    let Some(delimiter) = frame.iter().position(|byte| *byte == b'\n') else {
        return Err(ProtocolError::MissingDelimiter);
    };
    if delimiter != frame.len() - 1 {
        return if frame.iter().skip(delimiter + 1).any(|byte| *byte == b'\n') {
            Err(ProtocolError::MultipleFrames)
        } else {
            Err(ProtocolError::TrailingBytes)
        };
    }
    Ok(frame.split_at(delimiter).0)
}
