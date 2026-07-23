use crate::emergency_manifest::RuntimeOwnerProof;

#[cfg(any(target_os = "android", target_os = "linux"))]
use super::super::platform::{boot_id, process_role_matches, process_start_ticks};
use super::{RuntimeOwnerMismatch, RuntimeOwnerVerdict, RuntimeOwnerVerificationError};

#[allow(
    clippy::unnecessary_wraps,
    reason = "macOS keeps the same fallible owner-proof API while Android/Linux probes can fail"
)]
pub(super) fn verify_live_process(
    proof: &RuntimeOwnerProof,
) -> Result<RuntimeOwnerVerdict, RuntimeOwnerVerificationError> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        match process_start_ticks(proof.pid()).map_err(RuntimeOwnerVerificationError::Platform)? {
            None => {
                return Ok(RuntimeOwnerVerdict::Invalid(
                    RuntimeOwnerMismatch::ProcessMissing,
                ));
            }
            Some(start_ticks) if start_ticks != proof.start_ticks() => {
                return Ok(RuntimeOwnerVerdict::Invalid(
                    RuntimeOwnerMismatch::ProcessStartTicks,
                ));
            }
            Some(_) => {}
        }
        if !process_role_matches(proof.pid(), "ucloned")
            .map_err(RuntimeOwnerVerificationError::Platform)?
        {
            return Ok(RuntimeOwnerVerdict::Invalid(
                RuntimeOwnerMismatch::ProcessRole,
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if proof.pid() != std::process::id() || proof.start_ticks() != 1 {
            return Ok(RuntimeOwnerVerdict::Invalid(
                RuntimeOwnerMismatch::ProcessStartTicks,
            ));
        }
    }
    Ok(RuntimeOwnerVerdict::Valid)
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "macOS keeps the same fallible owner-proof API while Android/Linux boot reads can fail"
)]
pub(super) fn current_boot_id() -> Result<String, RuntimeOwnerVerificationError> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        boot_id().map_err(RuntimeOwnerVerificationError::Platform)
    }
    #[cfg(target_os = "macos")]
    {
        Ok(super::super::platform::boot_id())
    }
}
