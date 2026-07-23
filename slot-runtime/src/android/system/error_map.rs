use crate::android::CommandError;
use crate::bridge::{BridgeError, BridgeErrorCode};

use super::executor::ProcessFailure;

pub(super) const fn bridge(error: &BridgeError) -> CommandError {
    if matches!(
        error.code(),
        BridgeErrorCode::RunnerUnavailable | BridgeErrorCode::BuildMismatch
    ) {
        CommandError::StartFailed
    } else {
        CommandError::Rejected
    }
}

pub(super) const fn launch_bridge(error: &BridgeError) -> CommandError {
    match error.code() {
        BridgeErrorCode::LaunchEntryNotFound => CommandError::LaunchEntryNotFound,
        BridgeErrorCode::IdentityChanged => CommandError::IdentityChanged,
        BridgeErrorCode::PackageStateChanged | BridgeErrorCode::PendingSession => {
            CommandError::PackageStateChanged
        }
        BridgeErrorCode::RunnerUnavailable | BridgeErrorCode::BuildMismatch => {
            CommandError::StartFailed
        }
        _ => CommandError::Rejected,
    }
}

pub(super) const fn contract_bridge(error: &BridgeError) -> CommandError {
    match error.code() {
        BridgeErrorCode::IdentityChanged => CommandError::IdentityChanged,
        BridgeErrorCode::PackageStateChanged | BridgeErrorCode::PendingSession => {
            CommandError::PackageStateChanged
        }
        BridgeErrorCode::RunnerUnavailable | BridgeErrorCode::BuildMismatch => {
            CommandError::StartFailed
        }
        _ => CommandError::Rejected,
    }
}

pub(super) const fn process(error: &ProcessFailure) -> CommandError {
    match error {
        ProcessFailure::Io(_) => CommandError::StartFailed,
        ProcessFailure::TimedOut | ProcessFailure::OutputTooLarge { .. } => CommandError::Rejected,
    }
}
