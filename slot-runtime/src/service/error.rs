use crate::protocol::ErrorCode;

/// Stable service-level failure classes safe to expose on the daemon protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    /// A typed command violated the fixed Preview contract.
    #[error("invalid service request")]
    InvalidRequest,
    /// A package outside the compiled allowlist reached the service boundary.
    #[error("package is not allowlisted")]
    PackageNotAllowed,
    /// A Direct Boot package requires explicit unlocked-only Preview acceptance.
    #[error("Direct Boot conditional support requires explicit confirmation")]
    DirectBootConfirmationRequired,
    /// No enrollment or fixed slot exists for the command.
    #[error("service state was not found")]
    NotFound,
    /// A valid durable state conflicts with the requested mutation.
    #[error("service state conflicts with the request")]
    Conflict,
    /// Durable state or a visible view cannot be proved safe.
    #[error("recovery is required")]
    RecoveryRequired,
    /// Another mutating command currently owns the daemon gate.
    #[error("service is busy")]
    Busy,
    /// Installed identity no longer owns the enrolled data.
    #[error("package is quarantined")]
    Quarantined,
    /// User-zero credential-encrypted data is unavailable.
    #[error("user zero is locked")]
    UserLocked,
    /// The device does not meet the Preview runtime contract.
    #[error("device is unsupported")]
    UnsupportedDevice,
    /// A bounded internal operation failed before a more specific class was known.
    #[error("internal service failure")]
    Internal,
}

/// Failure boundary for enrollment before versus during durable publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentPublicationError {
    /// No enrollment unit was published, so the pending attempt may be exact-aborted.
    Unpublished(ServiceError),
    /// Publication may have started, so recovery must retain containment and evidence.
    PublicationAmbiguous,
}

impl ServiceError {
    /// Returns the stable daemon protocol code without exposing backend diagnostics.
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::InvalidRequest => ErrorCode::InvalidRequest,
            Self::PackageNotAllowed => ErrorCode::PackageNotAllowed,
            Self::DirectBootConfirmationRequired => ErrorCode::DirectBootConfirmationRequired,
            Self::NotFound => ErrorCode::NotFound,
            Self::Conflict => ErrorCode::Conflict,
            Self::RecoveryRequired => ErrorCode::RecoveryRequired,
            Self::Busy => ErrorCode::Busy,
            Self::Quarantined => ErrorCode::Quarantined,
            Self::UserLocked => ErrorCode::UserLocked,
            Self::UnsupportedDevice => ErrorCode::UnsupportedDevice,
            Self::Internal => ErrorCode::Internal,
        }
    }

    pub(super) const fn after_gate(self) -> Self {
        match self {
            Self::Quarantined | Self::RecoveryRequired => self,
            Self::InvalidRequest
            | Self::PackageNotAllowed
            | Self::DirectBootConfirmationRequired
            | Self::NotFound
            | Self::Conflict
            | Self::Busy
            | Self::UserLocked
            | Self::UnsupportedDevice
            | Self::Internal => Self::RecoveryRequired,
        }
    }
}
