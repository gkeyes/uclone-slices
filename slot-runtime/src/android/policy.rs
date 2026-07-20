use crate::domain::{ManagedPackage, PackageName, UserId};
use crate::runtime::PlatformError;

pub(super) const ALLOWLISTED_PACKAGE: &str = crate::target::PACKAGE;

#[derive(Debug, Clone, Copy)]
pub(super) enum Stage {
    Observe,
    CaptureGate,
    AcquireGate,
    VerifyGate,
    Quiesce,
    ApplyView,
    VerifyView,
    RestoreGate,
    RetireLease,
}

pub(super) fn ensure_supported(
    package: &ManagedPackage,
    stage: Stage,
) -> Result<(), PlatformError> {
    if package.user_id() != UserId::PRIMARY {
        return Err(failure(stage, "user_not_supported"));
    }
    ensure_package_supported(package.package_name(), stage)
}

pub(super) fn ensure_package_supported(
    package: &PackageName,
    stage: Stage,
) -> Result<(), PlatformError> {
    if package.as_str() != ALLOWLISTED_PACKAGE {
        return Err(failure(stage, "package_not_allowlisted"));
    }
    Ok(())
}

pub(super) fn failure(stage: Stage, detail: &str) -> PlatformError {
    let detail = detail.to_owned();
    match stage {
        Stage::Observe => PlatformError::ObservePackage { detail },
        Stage::CaptureGate => PlatformError::CaptureGateSnapshot { detail },
        Stage::AcquireGate => PlatformError::AcquireGate { detail },
        Stage::VerifyGate => PlatformError::VerifyGateHeld { detail },
        Stage::Quiesce => PlatformError::QuiesceProcesses { detail },
        Stage::ApplyView => PlatformError::ApplySlotView { detail },
        Stage::VerifyView => PlatformError::VerifySlotView { detail },
        Stage::RestoreGate => PlatformError::RestoreGate { detail },
        Stage::RetireLease => PlatformError::RetireGateLease { detail },
    }
}
