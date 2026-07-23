use crate::android::CommandError;
use crate::bridge::{BridgeError, BridgeErrorCode};

use super::executor::ProcessFailure;

pub(super) fn bridge(error: &BridgeError) -> CommandError {
    if error.code() == BridgeErrorCode::RunnerUnavailable {
        CommandError::StartFailed
    } else {
        CommandError::Rejected
    }
}

pub(super) fn launch_bridge(error: &BridgeError) -> CommandError {
    match error.code() {
        BridgeErrorCode::LaunchEntryNotFound => CommandError::LaunchEntryNotFound,
        BridgeErrorCode::IdentityChanged => CommandError::IdentityChanged,
        BridgeErrorCode::PackageStateChanged | BridgeErrorCode::PendingSession => {
            CommandError::PackageStateChanged
        }
        BridgeErrorCode::RunnerUnavailable => CommandError::StartFailed,
        _ => CommandError::Rejected,
    }
}

pub(super) fn contract_bridge(error: &BridgeError) -> CommandError {
    match error.code() {
        BridgeErrorCode::IdentityChanged => CommandError::IdentityChanged,
        BridgeErrorCode::PackageStateChanged | BridgeErrorCode::PendingSession => {
            CommandError::PackageStateChanged
        }
        BridgeErrorCode::RunnerUnavailable => CommandError::StartFailed,
        _ => CommandError::Rejected,
    }
}

pub(super) const fn process(error: &ProcessFailure) -> CommandError {
    match error {
        ProcessFailure::Io(_) => CommandError::StartFailed,
        ProcessFailure::TimedOut | ProcessFailure::OutputTooLarge { .. } => CommandError::Rejected,
    }
}
