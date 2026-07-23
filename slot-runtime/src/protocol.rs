#![doc = "Bounded versioned JSON-lines control-plane protocol for the Slots Preview runtime."]

use std::fmt;

mod client;
mod framing;
mod payload;
mod request;
mod response;

pub use client::{DEFAULT_CLIENT_TIMEOUT, UnixClient, read_response, write_request};
pub use payload::{
    Ack, AckOperation, LaunchResult, LaunchStatus, ManagedAppSummary, ManagedAppsReport,
    PackageInspectionReport, PackageSnapshotReport, PackageStatus, ProbeReport, ReconcileOutcome,
    ReconcileReport, RecoveryTargetsReport, ResponsePayload, SlotSummary, SlotsReport,
    SwitchResult,
};
pub use request::{Command, Request, RequestId, decode_request, encode_request};
pub use response::{ErrorCode, Response, ResponseStatus, decode_response, encode_response};

use framing::{encode_json_line, frame_line};

/// Current wire schema version.
pub const SCHEMA_VERSION: u32 = 2;

/// Build identity that must match the manager APK before mutations are enabled.
pub const RUNTIME_BUILD_ID: &str = match option_env!("UCLONE_PREVIEW_BUILD_ID") {
    Some(value) => value,
    None => "development",
};
/// Maximum encoded request or response frame, including its newline delimiter.
pub const MAX_FRAME_SIZE: usize = 16 * 1024;
/// Legacy single-target profile package retained only for fixed regression fixtures.
pub const ALLOWED_PACKAGE: &str = crate::target::PACKAGE;

/// Protocol parsing, validation, framing, and transport failures.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// The encoded frame exceeded [`MAX_FRAME_SIZE`].
    #[error("frame too large: {size} bytes (maximum {MAX_FRAME_SIZE})")]
    FrameTooLarge {
        /// Actual encoded frame size.
        size: usize,
    },
    /// A JSON-lines frame did not end in exactly one newline delimiter.
    #[error("frame is missing its final newline delimiter")]
    MissingDelimiter,
    /// A frame contained more than one JSON-lines record.
    #[error("frame contains multiple records")]
    MultipleFrames,
    /// Bytes followed the first record without forming a complete second record.
    #[error("frame contains trailing bytes")]
    TrailingBytes,
    /// The JSON payload could not be decoded.
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A Unix stream operation failed.
    #[error("UnixStream I/O: {0}")]
    Io(#[from] std::io::Error),
    /// The request or response used an unsupported schema version.
    #[error("unsupported schema version: {0}")]
    UnsupportedSchema(u32),
    /// A request id was empty or contained unsafe characters.
    #[error("invalid request id: {0}")]
    InvalidRequestId(String),
    /// The client and Runtime were not produced by the same paired build.
    #[error("client and Runtime build identities do not match")]
    RuntimePairMismatch,
    /// A package name was not present where required.
    #[error("missing package field")]
    MissingPackage,
    /// A slot id was not present where required.
    #[error("missing slot field")]
    MissingSlot,
    /// A field was supplied for a command that does not accept it.
    #[error("field is not valid for this command: {0}")]
    UnexpectedField(&'static str),
    /// The command name is not part of the fixed protocol.
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    /// A package is valid as an Android identifier but is not allowlisted.
    #[error("package is not allowlisted: {0}")]
    PackageNotAllowed(String),
    /// A slot is valid as an identifier but is not compiled into this artifact.
    #[error("slot is not allowlisted: {0}")]
    SlotNotAllowed(String),
    /// A response had an invalid status/error combination.
    #[error("invalid response status/error combination")]
    InvalidResponse,
    /// The response did not correspond to the request sent on this stream.
    #[error("response request id mismatch: expected {expected}, got {actual}")]
    RequestIdMismatch {
        /// Request id sent by the client.
        expected: RequestId,
        /// Request id echoed by the peer.
        actual: RequestId,
    },
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
