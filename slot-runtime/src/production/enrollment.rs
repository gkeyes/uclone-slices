use crate::android::PackageProbe;
use crate::domain::{ManagedPackage, PackageKey, SlotId, SlotView};
use crate::lifecycle::LifecycleState;
use crate::materializer::{BaseAnchor, MaterializationBackend};
use crate::protocol::ALLOWED_PACKAGE;
use crate::service::ServiceError;

pub(super) fn initial_managed<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
) -> Result<ManagedPackage, ServiceError> {
    require_key(key)?;
    if !probe
        .user0_unlocked()
        .map_err(|_| ServiceError::UnsupportedDevice)?
    {
        return Err(ServiceError::UserLocked);
    }
    let namespace = probe
        .mount_namespace_proof()
        .map_err(|_| ServiceError::UnsupportedDevice)?;
    if !namespace.is_global() {
        return Err(ServiceError::UnsupportedDevice);
    }
    let observation = probe
        .observe_package(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::Internal)?;
    let base = observation.package_manager_inodes();
    if observation.pending_install()
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
    let preview = SlotId::parse(crate::target::PREVIEW_SLOT).map_err(|_| ServiceError::Internal)?;
    if probe
        .slot_inodes(key.package_name(), &preview)
        .map_err(|_| ServiceError::RecoveryRequired)?
        .is_some()
    {
        return Err(ServiceError::RecoveryRequired);
    }
    ManagedPackage::new(
        key.clone(),
        observation.identity().clone(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .map_err(|_| ServiceError::RecoveryRequired)
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
    if key.user_id() == crate::domain::UserId::PRIMARY
        && key.package_name().as_str() == ALLOWED_PACKAGE
    {
        Ok(())
    } else {
        Err(ServiceError::PackageNotAllowed)
    }
}
