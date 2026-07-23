use crate::android::ProbeError;
use crate::bridge::{BridgeError, BridgeErrorCode, PackageSnapshot};
use crate::domain::{AppIdentity, DataInodes, PackageCandidate, PackageCompatibility};

pub(super) fn candidate_from_snapshot(
    snapshot: &PackageSnapshot,
) -> Result<PackageCandidate, ProbeError> {
    let identity = AppIdentity::new(
        snapshot.uid(),
        snapshot.signature_sha256(),
        snapshot.version_code(),
        snapshot.code_path(),
    )
    .map_err(|_| ProbeError::InvalidResponse)?;
    let inodes = DataInodes::new(
        snapshot.package_manager_ce_inode(),
        snapshot.package_manager_de_inode(),
    )
    .map_err(|_| ProbeError::InvalidResponse)?;
    Ok(PackageCandidate::new(
        identity,
        inodes,
        snapshot.pending_install(),
        PackageCompatibility::new(
            snapshot.system_app(),
            snapshot.shared_uid(),
            snapshot.direct_boot_aware(),
        ),
    ))
}

pub(super) const fn map_fact_error(error: super::FactError) -> ProbeError {
    match error {
        super::FactError::Unavailable => ProbeError::Unavailable,
        super::FactError::Invalid => ProbeError::InvalidResponse,
    }
}

pub(super) const fn map_bridge_error(error: &BridgeError) -> ProbeError {
    match error.code() {
        BridgeErrorCode::RunnerUnavailable
        | BridgeErrorCode::TimedOut
        | BridgeErrorCode::BuildMismatch => ProbeError::Unavailable,
        _ => ProbeError::InvalidResponse,
    }
}
