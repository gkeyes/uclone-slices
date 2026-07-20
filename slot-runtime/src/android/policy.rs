use crate::domain::{ManagedPackage, UserId};
use crate::runtime::PlatformError;

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
