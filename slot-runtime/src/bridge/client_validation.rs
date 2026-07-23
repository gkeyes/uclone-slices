use super::{
    ALLOWED_USER_ID, BridgeError, BridgeErrorCode, BridgePayload, BridgeRunnerError,
    PackageSnapshot,
};
use crate::domain::PackageName;

pub(super) fn require_package(package: &str) -> Result<PackageName, BridgeError> {
    PackageName::parse(package).map_err(|_| {
        BridgeError::new(
            BridgeErrorCode::PackageNotAllowed,
            "package identifier is not valid",
        )
    })
}

pub(super) fn require_user(user_id: u32) -> Result<(), BridgeError> {
    if user_id == ALLOWED_USER_ID {
        Ok(())
    } else {
        Err(BridgeError::new(
            BridgeErrorCode::UserNotAllowed,
            "only Android user 0 is supported",
        ))
    }
}

pub(super) fn validate_package_snapshot(
    snapshot: PackageSnapshot,
    expected: &PackageName,
) -> Result<PackageSnapshot, BridgeError> {
    let observed = require_package(snapshot.package_name())?;
    if &observed != expected {
        return Err(BridgeError::new(
            BridgeErrorCode::RequestMismatch,
            "package response does not match request",
        ));
    }
    require_user(snapshot.user_id())?;
    Ok(snapshot)
}

pub(super) fn require_ack(payload: &BridgePayload) -> Result<(), BridgeError> {
    if matches!(payload, BridgePayload::Ack(_)) {
        Ok(())
    } else {
        Err(BridgeError::new(
            BridgeErrorCode::InvalidResponse,
            "mutation returned the wrong payload type",
        ))
    }
}

pub(super) const fn payload_name(payload: &BridgePayload) -> &'static str {
    match payload {
        BridgePayload::Device(_) => "device",
        BridgePayload::Package(_) => "package",
        BridgePayload::Gate(_) => "gate",
        BridgePayload::Ack(_) => "ack",
    }
}

pub(super) fn map_runner_error(error: &BridgeRunnerError) -> BridgeError {
    match error {
        BridgeRunnerError::Io(_) => BridgeError::new(
            BridgeErrorCode::RunnerUnavailable,
            "fixed app_process could not be executed",
        ),
        BridgeRunnerError::NonZeroExit { .. } => BridgeError::new(
            BridgeErrorCode::CommandFailed,
            "fixed app_process returned a non-zero status",
        ),
        BridgeRunnerError::TimedOut => BridgeError::new(
            BridgeErrorCode::TimedOut,
            "fixed app_process exceeded the five-second deadline",
        ),
        BridgeRunnerError::OutputTooLarge { size } => BridgeError::new(
            BridgeErrorCode::ResponseTooLarge,
            format!("fixed app_process response is {size} bytes"),
        ),
        BridgeRunnerError::BuildMismatch => BridgeError::new(
            BridgeErrorCode::BuildMismatch,
            "fixed app_process bridge is not paired with this Runtime",
        ),
        BridgeRunnerError::InvalidHandshake => BridgeError::new(
            BridgeErrorCode::InvalidResponse,
            "fixed app_process bridge returned an invalid startup handshake",
        ),
    }
}
