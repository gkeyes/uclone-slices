use crate::android::PackageProbe;
use crate::domain::{
    ManagedPackage, PackageCandidate, PackageCompatibility, PackageKey, PackageSupportLevel,
    SlotId, SlotView,
};
use crate::lifecycle::LifecycleState;
use crate::materializer::{BaseAnchor, MaterializationBackend};
use crate::service::ServiceError;

pub(super) fn initial_managed<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
) -> Result<ManagedPackage, ServiceError> {
    let package = candidate_managed(probe, key)?;
    verify_candidate(probe, key, &package)?;
    Ok(package)
}

pub(super) fn inspect_candidate<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
) -> Result<(ManagedPackage, PackageCompatibility), ServiceError> {
    let candidate = read_candidate(probe, key)?;
    let compatibility = candidate.compatibility();
    Ok((managed_from_candidate(key, &candidate)?, compatibility))
}

pub(super) fn candidate_managed<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
) -> Result<ManagedPackage, ServiceError> {
    let candidate = read_candidate(probe, key)?;
    managed_from_candidate(key, &candidate)
}

fn read_candidate<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
) -> Result<PackageCandidate, ServiceError> {
    require_key(key)?;
    if !probe
        .user0_unlocked()
        .map_err(|_| ServiceError::UnsupportedDevice)?
    {
        return Err(ServiceError::UserLocked);
    }
    let candidate = probe
        .inspect_package(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::Internal)?;
    if candidate.pending_install() {
        return Err(ServiceError::RecoveryRequired);
    }
    if candidate.compatibility().support_level() == PackageSupportLevel::Blocked {
        return Err(ServiceError::PackageNotAllowed);
    }
    Ok(candidate)
}

fn managed_from_candidate(
    key: &PackageKey,
    candidate: &PackageCandidate,
) -> Result<ManagedPackage, ServiceError> {
    let base = candidate.package_manager_inodes();
    ManagedPackage::new(
        key.clone(),
        candidate.identity().clone(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .map_err(|_| ServiceError::RecoveryRequired)
}

fn verify_candidate<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
    package: &ManagedPackage,
) -> Result<(), ServiceError> {
    let namespace = probe
        .mount_namespace_proof()
        .map_err(|_| ServiceError::UnsupportedDevice)?;
    if !namespace.is_global() {
        return Err(ServiceError::UnsupportedDevice);
    }
    let observation = probe
        .observe_package(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let base = package.base_inodes();
    if observation.identity() != package.identity()
        || observation.package_manager_inodes() != base
        || observation.pending_install()
        || observation.canonical_inodes() != base
        || observation.active_process_inodes() != base
    {
        return Err(ServiceError::RecoveryRequired);
    }
    let proof = probe
        .view_proof(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let counts = proof.canonical().mount_counts();
    if proof.canonical().inodes() != base
        || proof.mirror_inodes() != base
        || proof.zygote_inodes() != base
        || counts.ce() != 0
        || counts.de() != 0
    {
        return Err(ServiceError::RecoveryRequired);
    }
    Ok(())
}

pub(super) fn capture_base<M: MaterializationBackend>(
    materializer: &mut M,
    package: &ManagedPackage,
) -> Result<BaseAnchor, ServiceError> {
    materializer
        .verify_gate_held(package)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    materializer
        .verify_processes_quiesced(package)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let base = materializer
        .capture_base_anchor(package)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if base.inodes() != package.base_inodes()
        || base.security().ce().uid() != package.identity().uid()
        || base.security().de().uid() != package.identity().uid()
    {
        return Err(ServiceError::RecoveryRequired);
    }
    Ok(base)
}

pub(super) fn require_key(key: &PackageKey) -> Result<(), ServiceError> {
    if key.user_id() == crate::domain::UserId::PRIMARY {
        Ok(())
    } else {
        Err(ServiceError::PackageNotAllowed)
    }
}
