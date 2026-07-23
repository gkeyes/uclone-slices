use std::fmt;

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
    /// The child emitted more than the response budget.
    ResponseTooLarge,
    /// The fixed app_process could not be started or read.
    RunnerUnavailable,
    /// The fixed app_process exited unsuccessfully.
    CommandFailed,
    /// The fixed app_process exceeded its execution deadline.
    TimedOut,
    /// The device is credential locked for a command that requires unlock.
    DeviceLocked,
    /// PackageManager did not report the package.
    PackageNotFound,
    /// The package has no enabled launcher entry for user zero.
    LaunchEntryNotFound,
    /// UID or signing digest no longer matches the enrolled identity.
    IdentityChanged,
    /// Version, code path, Base inodes, or install-session state changed.
    PackageStateChanged,
    /// PackageManager reported a pending install session.
    PendingSession,
    /// The Java bridge artifact does not match the Runtime build identity.
    BuildMismatch,
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
            Self::LaunchEntryNotFound => "launch_entry_not_found",
            Self::IdentityChanged => "identity_changed",
            Self::PackageStateChanged => "package_state_changed",
            Self::PendingSession => "pending_session",
            Self::BuildMismatch => "build_mismatch",
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

pub(super) fn invalid_response(message: impl Into<String>) -> BridgeError {
    BridgeError::new(BridgeErrorCode::InvalidResponse, message)
}
