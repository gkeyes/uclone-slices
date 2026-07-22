use serde::{Deserialize, Serialize, de::Deserializer};

use super::payload::ResponsePayload;
use super::{ProtocolError, RequestId, SCHEMA_VERSION, encode_json_line, frame_line};

/// Response status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    /// The command completed successfully.
    Ok,
    /// The command was rejected or could not complete.
    Error,
}

/// Stable machine-readable response error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Request framing or fields were malformed.
    InvalidRequest,
    /// The request schema is newer than this runtime.
    UnsupportedSchema,
    /// The package is outside the initial allowlist.
    PackageNotAllowed,
    /// Direct Boot conditional support was not explicitly acknowledged.
    DirectBootConfirmationRequired,
    /// The package or slot is not enrolled/present.
    NotFound,
    /// The requested operation conflicts with durable state.
    Conflict,
    /// Reconciliation is required before another operation.
    RecoveryRequired,
    /// The runtime is busy with another operation.
    Busy,
    /// The package identity no longer owns its enrolled data.
    Quarantined,
    /// User 0 is still credential-locked for CE operations.
    UserLocked,
    /// The device does not satisfy the Preview runtime contract.
    UnsupportedDevice,
    /// An internal runtime failure occurred.
    Internal,
}

/// A typed response envelope echoed to the requesting client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Response {
    schema_version: u32,
    request_id: RequestId,
    status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<ErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<ResponsePayload>,
}

impl Response {
    /// Creates a successful response with a required typed payload.
    pub fn ok(request_id: RequestId, payload: ResponsePayload) -> Result<Self, ProtocolError> {
        payload.validate()?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            request_id,
            status: ResponseStatus::Ok,
            error_code: None,
            payload: Some(payload),
        })
    }

    /// Creates a failed response without a success payload.
    pub const fn error(request_id: RequestId, error_code: ErrorCode) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            request_id,
            status: ResponseStatus::Error,
            error_code: Some(error_code),
            payload: None,
        }
    }

    /// Returns the echoed request id.
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Returns the response status.
    pub const fn status(&self) -> ResponseStatus {
        self.status
    }

    /// Returns the optional typed failure code.
    pub const fn error_code(&self) -> Option<ErrorCode> {
        self.error_code
    }

    /// Returns the required success payload, when this response is successful.
    pub const fn payload(&self) -> Option<&ResponsePayload> {
        self.payload.as_ref()
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireResponse::deserialize(deserializer)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(ProtocolError::UnsupportedSchema(
                wire.schema_version,
            )));
        }
        validate_parts(wire.status, wire.error_code, wire.payload, wire.request_id)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResponse {
    schema_version: u32,
    request_id: RequestId,
    status: ResponseStatus,
    #[serde(default)]
    error_code: Option<ErrorCode>,
    #[serde(default)]
    payload: Option<ResponsePayload>,
}

fn validate_parts(
    status: ResponseStatus,
    error_code: Option<ErrorCode>,
    payload: Option<ResponsePayload>,
    request_id: RequestId,
) -> Result<Response, ProtocolError> {
    match (status, error_code, payload) {
        (ResponseStatus::Ok, None, Some(payload)) => {
            payload.validate()?;
            Ok(Response {
                schema_version: SCHEMA_VERSION,
                request_id,
                status,
                error_code: None,
                payload: Some(payload),
            })
        }
        (ResponseStatus::Error, Some(error_code), None) => Ok(Response {
            schema_version: SCHEMA_VERSION,
            request_id,
            status,
            error_code: Some(error_code),
            payload: None,
        }),
        _ => Err(ProtocolError::InvalidResponse),
    }
}

/// Encodes one response as a bounded JSON-lines frame.
pub fn encode_response(response: &Response) -> Result<Vec<u8>, ProtocolError> {
    encode_json_line(response)
}

/// Decodes one response frame.
pub fn decode_response(frame: &[u8]) -> Result<Response, ProtocolError> {
    let line = frame_line(frame)?;
    let mut deserializer = serde_json::Deserializer::from_slice(line);
    let wire = WireResponse::deserialize(&mut deserializer)?;
    deserializer.end().map_err(ProtocolError::Json)?;
    if wire.schema_version != SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchema(wire.schema_version));
    }
    validate_parts(wire.status, wire.error_code, wire.payload, wire.request_id)
}
