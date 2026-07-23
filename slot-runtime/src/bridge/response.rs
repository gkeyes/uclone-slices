use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{
    BRIDGE_SCHEMA_VERSION, BridgeError, BridgeErrorCode, BridgePayload,
    LEGACY_BRIDGE_SCHEMA_VERSION, invalid_response,
};

/// Strict bridge response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_id: Option<String>,
    request_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<BridgePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<BridgeErrorCode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BridgeResponseWire {
    schema_version: u32,
    #[serde(default)]
    build_id: Option<String>,
    request_id: String,
    ok: bool,
    #[serde(default)]
    payload: Option<BridgePayload>,
    #[serde(default)]
    error_code: Option<BridgeErrorCode>,
}

impl<'de> Deserialize<'de> for BridgeResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = BridgeResponseWire::deserialize(deserializer)?;
        Self::from_wire(wire).map_err(D::Error::custom)
    }
}

impl BridgeResponse {
    /// Builds a valid success envelope.
    pub fn ok(request_id: &str, payload: BridgePayload) -> Result<Self, BridgeError> {
        Self::from_wire(BridgeResponseWire {
            schema_version: BRIDGE_SCHEMA_VERSION,
            build_id: Some(crate::protocol::RUNTIME_BUILD_ID.to_owned()),
            request_id: request_id.to_owned(),
            ok: true,
            payload: Some(payload),
            error_code: None,
        })
    }

    /// Builds a valid error envelope.
    pub fn error(request_id: &str, code: BridgeErrorCode) -> Result<Self, BridgeError> {
        Self::from_wire(BridgeResponseWire {
            schema_version: BRIDGE_SCHEMA_VERSION,
            build_id: Some(crate::protocol::RUNTIME_BUILD_ID.to_owned()),
            request_id: request_id.to_owned(),
            ok: false,
            payload: None,
            error_code: Some(code),
        })
    }

    /// Decodes exactly one bounded JSON response.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BridgeError> {
        if bytes.len() > super::MAX_OUTPUT_BYTES {
            return Err(BridgeError::new(
                BridgeErrorCode::ResponseTooLarge,
                format!("bridge response is {} bytes", bytes.len()),
            ));
        }
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let wire = BridgeResponseWire::deserialize(&mut deserializer)
            .map_err(|error| invalid_response(format!("invalid bridge JSON: {error}")))?;
        deserializer
            .end()
            .map_err(|error| invalid_response(format!("trailing bridge JSON: {error}")))?;
        Self::from_wire(wire)
    }

    fn from_wire(wire: BridgeResponseWire) -> Result<Self, BridgeError> {
        validate_envelope_version(wire.schema_version, wire.build_id.as_deref())?;
        super::validation::validate_request_id(&wire.request_id)?;
        match (wire.ok, wire.payload, wire.error_code) {
            (true, Some(payload), None) => Ok(Self {
                schema_version: wire.schema_version,
                build_id: wire.build_id,
                request_id: wire.request_id,
                ok: true,
                payload: Some(payload),
                error_code: None,
            }),
            (false, None, Some(error_code)) => Ok(Self {
                schema_version: wire.schema_version,
                build_id: wire.build_id,
                request_id: wire.request_id,
                ok: false,
                payload: None,
                error_code: Some(error_code),
            }),
            _ => Err(invalid_response(
                "bridge ok/payload/errorCode shape mismatch",
            )),
        }
    }

    /// Returns the schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the paired bridge build identity for schema v2.
    pub fn build_id(&self) -> Option<&str> {
        self.build_id.as_deref()
    }

    /// Returns the echoed request id.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Returns whether the bridge operation succeeded.
    pub const fn is_ok(&self) -> bool {
        self.ok
    }

    /// Returns the success payload, if present.
    pub const fn payload(&self) -> Option<&BridgePayload> {
        self.payload.as_ref()
    }

    /// Returns the remote stable error code, if present.
    pub const fn error_code(&self) -> Option<BridgeErrorCode> {
        self.error_code
    }

    pub(crate) fn require_compatible(
        &self,
        expected_build_id: &str,
        allow_legacy_v1: bool,
    ) -> Result<(), BridgeError> {
        match (self.schema_version, self.build_id()) {
            (LEGACY_BRIDGE_SCHEMA_VERSION, None) if allow_legacy_v1 => Ok(()),
            (BRIDGE_SCHEMA_VERSION, Some(build_id)) if build_id == expected_build_id => Ok(()),
            _ => Err(BridgeError::new(
                BridgeErrorCode::BuildMismatch,
                "Java bridge is not paired with this Runtime",
            )),
        }
    }

    pub(crate) fn require_paired_v2(&self, expected_build_id: &str) -> Result<(), BridgeError> {
        self.require_compatible(expected_build_id, false)
    }
}

fn validate_envelope_version(
    schema_version: u32,
    build_id: Option<&str>,
) -> Result<(), BridgeError> {
    match (schema_version, build_id) {
        (LEGACY_BRIDGE_SCHEMA_VERSION, None) => Ok(()),
        (BRIDGE_SCHEMA_VERSION, Some(value)) if valid_build_id(value) => Ok(()),
        (BRIDGE_SCHEMA_VERSION, Some(_)) => Err(invalid_response("invalid bridge buildId")),
        (BRIDGE_SCHEMA_VERSION, None) => Err(invalid_response("missing bridge buildId")),
        (LEGACY_BRIDGE_SCHEMA_VERSION, Some(_)) => {
            Err(invalid_response("legacy bridge must not carry buildId"))
        }
        _ => Err(invalid_response("unsupported bridge schemaVersion")),
    }
}

fn valid_build_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Decodes one strict bridge response frame.
pub fn decode_response(bytes: &[u8]) -> Result<BridgeResponse, BridgeError> {
    BridgeResponse::from_bytes(bytes)
}
