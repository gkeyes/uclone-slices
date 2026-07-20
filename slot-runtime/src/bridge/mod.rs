#![doc = "Fail-closed, fixed-command Android app_process bridge for package facts."]
#![allow(clippy::doc_markdown)]

mod client;
mod device;
mod payload;
mod protocol;
mod response;
mod runner;
mod validation;

pub use client::BridgeClient;
pub use device::DeviceSnapshot;
pub use payload::{AckPayload, BridgeGateSnapshot, BridgePayload};
pub use protocol::PackageSnapshot;
pub use response::{BridgeResponse, decode_response};
pub use runner::{AppProcessRunner, BridgeCommandRunner, BridgeRunnerError};

use std::fmt;

pub use crate::domain::PackageEnabledState;
use crate::domain::PackageName;

/// Current bridge response schema.
pub const BRIDGE_SCHEMA_VERSION: u32 = 1;
/// Maximum response size accepted from the app_process child, including no
/// implicit framing bytes.
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024;
/// The only package and Android user addressable by this preview bridge.
pub const ALLOWED_PACKAGE: &str = crate::target::PACKAGE;
/// The only Android user addressable by this preview bridge.
pub const ALLOWED_USER_ID: u32 = crate::target::USER_ID;
/// Fixed app_process executable. This is deliberately not configurable.
pub const APP_PROCESS_PATH: &str = "/system/bin/app_process";
/// Fixed bridge APK classpath. This is deliberately not configurable.
pub const BRIDGE_CLASSPATH: &str = crate::target::BRIDGE_CLASSPATH;
/// Fixed Java entry point loaded by app_process.
pub const BRIDGE_MAIN_CLASS: &str = "com.uclone.slotbridge.Main";

/// The fixed bridge operation. It carries no caller-controlled strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeCommand {
    /// Read whether user 0's credential-encrypted device state is unlocked.
    DeviceStatus,
    /// Read a validated package's user 0 PackageManager state.
    PackageStatus(PackageName),
    /// Read only a validated package's enabled and suspended state.
    GateStatus(PackageName),
    /// Set a validated package's enabled-state enum for user 0.
    SetEnabled(PackageName, PackageEnabledState),
    /// Set a validated package's suspension state for user 0.
    SetSuspended(PackageName, bool),
}

impl BridgeCommand {
    /// Returns the exact argv passed after the fixed executable path.
    pub fn argv(&self) -> Vec<String> {
        match self {
            Self::DeviceStatus => fixed_args("probe-device"),
            Self::PackageStatus(package) => package_args("probe-package", package),
            Self::GateStatus(package) => package_args("probe-gate", package),
            Self::SetEnabled(package, state) => vec![
                "/system/bin".into(),
                BRIDGE_MAIN_CLASS.into(),
                "set-enabled".into(),
                package.as_str().into(),
                enabled_state_arg(*state).into(),
            ],
            Self::SetSuspended(package, suspended) => vec![
                "/system/bin".into(),
                BRIDGE_MAIN_CLASS.into(),
                "set-suspended".into(),
                package.as_str().into(),
                if *suspended { "true" } else { "false" }.into(),
            ],
        }
    }

    pub(crate) fn session_request(&self) -> Vec<u8> {
        let mut request = self
            .argv()
            .into_iter()
            .skip(2)
            .collect::<Vec<_>>()
            .join("\t")
            .into_bytes();
        request.push(b'\n');
        request
    }

    /// Returns the fixed request id echoed by the Java bridge.
    pub const fn request_id(&self) -> &'static str {
        match self {
            Self::DeviceStatus => "device",
            Self::PackageStatus(_) => "package",
            Self::GateStatus(_) => "gate",
            Self::SetEnabled(_, _) => "set-enabled",
            Self::SetSuspended(_, _) => "set-suspended",
        }
    }

    /// Returns the expected tagged payload name for this operation.
    pub const fn payload_name(&self) -> &'static str {
        match self {
            Self::DeviceStatus => "device",
            Self::PackageStatus(_) => "package",
            Self::GateStatus(_) => "gate",
            Self::SetEnabled(_, _) | Self::SetSuspended(_, _) => "ack",
        }
    }
}

fn fixed_args(operation: &str) -> Vec<String> {
    vec![
        "/system/bin".into(),
        BRIDGE_MAIN_CLASS.into(),
        operation.into(),
    ]
}

fn package_args(operation: &str, package: &PackageName) -> Vec<String> {
    vec![
        "/system/bin".into(),
        BRIDGE_MAIN_CLASS.into(),
        operation.into(),
        package.as_str().into(),
    ]
}

const fn enabled_state_arg(state: PackageEnabledState) -> &'static str {
    match state {
        PackageEnabledState::Default => "default",
        PackageEnabledState::Enabled => "enabled",
        PackageEnabledState::Disabled => "disabled",
        PackageEnabledState::DisabledUser => "disabled_user",
        PackageEnabledState::DisabledUntilUsed => "disabled_until_used",
    }
}

/// Stable machine-readable bridge failure code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeErrorCode {
    /// A locally supplied package, user, or command request was rejected.
    InvalidRequest,
    /// The package is outside the fixed allowlist.
    PackageNotAllowed,
    /// The user is outside the fixed user-0 boundary.
    UserNotAllowed,
    /// The response was not valid bridge JSON or failed semantic validation.
    InvalidResponse,
    /// The response request id did not match the fixed command.
    RequestMismatch,
    /// The child emitted more than the 16 KiB response budget.
    ResponseTooLarge,
    /// The fixed app_process could not be started or read.
    RunnerUnavailable,
    /// The fixed app_process exited unsuccessfully.
    CommandFailed,
    /// The fixed app_process exceeded its execution deadline.
    TimedOut,
    /// The device is credential locked for a command that requires unlock.
    DeviceLocked,
    /// PackageManager did not report the allowlisted package.
    PackageNotFound,
    /// PackageManager reported a pending install session.
    PendingSession,
    /// The bridge reported an otherwise unclassified failure.
    Internal,
}

impl BridgeErrorCode {
    /// Returns the stable wire spelling of this code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::PackageNotAllowed => "package_not_allowed",
            Self::UserNotAllowed => "user_not_allowed",
            Self::InvalidResponse => "invalid_response",
            Self::RequestMismatch => "request_mismatch",
            Self::ResponseTooLarge => "response_too_large",
            Self::RunnerUnavailable => "runner_unavailable",
            Self::CommandFailed => "command_failed",
            Self::TimedOut => "timed_out",
            Self::DeviceLocked => "device_locked",
            Self::PackageNotFound => "package_not_found",
            Self::PendingSession => "pending_session",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for BridgeErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Error returned by the typed bridge client.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct BridgeError {
    code: BridgeErrorCode,
    message: String,
}

impl BridgeError {
    pub(crate) fn new(code: BridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable code.
    pub const fn code(&self) -> BridgeErrorCode {
        self.code
    }

    /// Returns a human-readable diagnostic without exposing child shell text.
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub(crate) fn invalid_response(message: impl Into<String>) -> BridgeError {
    BridgeError::new(BridgeErrorCode::InvalidResponse, message)
}
