use crate::domain::{ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId};
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;

use super::{PackageSnapshot, PackageState, ServiceError};

pub(super) fn allowlisted_key() -> Result<PackageKey, ServiceError> {
    let package = PackageName::parse(ALLOWED_PACKAGE).map_err(|_| ServiceError::Internal)?;
    Ok(PackageKey::new(package, UserId::PRIMARY))
}

pub(super) fn package_key(package: &PackageName) -> Result<PackageKey, ServiceError> {
    if package.as_str() != ALLOWED_PACKAGE {
        return Err(ServiceError::PackageNotAllowed);
    }
    Ok(PackageKey::new(package.clone(), UserId::PRIMARY))
}

pub(super) fn snapshot(
    key: &PackageKey,
    state: PackageState,
) -> Result<PackageSnapshot, ServiceError> {
    match state {
        PackageState::Absent => Err(ServiceError::NotFound),
        PackageState::RecoveryRequired => Err(ServiceError::RecoveryRequired),
        PackageState::Quarantined => Err(ServiceError::Quarantined),
        PackageState::Ready(snapshot) => {
            validate_snapshot(key, &snapshot)?;
            Ok(*snapshot)
        }
    }
}

pub(super) const fn reportable(package: &ManagedPackage) -> Result<(), ServiceError> {
    match package.lifecycle_state() {
        LifecycleState::Quarantined => Err(ServiceError::Quarantined),
        LifecycleState::RecoveryRequired
        | LifecycleState::LifecycleDrifted
        | LifecycleState::RepairWaiting => Err(ServiceError::RecoveryRequired),
        LifecycleState::Normal
        | LifecycleState::UpdatePreparing
        | LifecycleState::UpdateWindowOpen
        | LifecycleState::UpdateVerifying => Ok(()),
    }
}

pub(super) const fn switchable(package: &ManagedPackage) -> Result<(), ServiceError> {
    match package.lifecycle_state() {
        LifecycleState::Normal => Ok(()),
        LifecycleState::Quarantined => Err(ServiceError::Quarantined),
        LifecycleState::RecoveryRequired
        | LifecycleState::LifecycleDrifted
        | LifecycleState::RepairWaiting => Err(ServiceError::RecoveryRequired),
        LifecycleState::UpdatePreparing
        | LifecycleState::UpdateWindowOpen
        | LifecycleState::UpdateVerifying => Err(ServiceError::Conflict),
    }
}

pub(super) fn enrolled(key: &PackageKey, package: &ManagedPackage) -> Result<(), ServiceError> {
    if package.package_name() != key.package_name()
        || package.user_id() != UserId::PRIMARY
        || !package.active_slot().is_base()
        || package.active_inodes() != package.base_inodes()
        || package.lifecycle_state() != LifecycleState::Normal
    {
        return Err(ServiceError::RecoveryRequired);
    }
    Ok(())
}

pub(super) fn requested_slot(slot: &SlotId) -> Result<(), ServiceError> {
    if slot.is_base() || slot.as_str() == crate::target::PREVIEW_SLOT {
        Ok(())
    } else {
        Err(ServiceError::NotFound)
    }
}

pub(super) fn preview_target(
    package: &ManagedPackage,
    target: &SlotView,
) -> Result<(), ServiceError> {
    if target.slot_id().as_str() != crate::target::PREVIEW_SLOT
        || shares_base_inode(package, target)
    {
        return Err(ServiceError::RecoveryRequired);
    }
    if target.inodes().ce() == target.inodes().de() {
        return Err(ServiceError::RecoveryRequired);
    }
    Ok(())
}

fn validate_snapshot(key: &PackageKey, snapshot: &PackageSnapshot) -> Result<(), ServiceError> {
    let managed = snapshot.managed();
    if managed.package_name() != key.package_name() || managed.user_id() != UserId::PRIMARY {
        return Err(ServiceError::RecoveryRequired);
    }
    if !managed.active_slot().is_base()
        && managed.active_slot().as_str() != crate::target::PREVIEW_SLOT
    {
        return Err(ServiceError::RecoveryRequired);
    }
    if let Some(preview) = snapshot.preview() {
        preview_target(managed, preview)?;
    }
    if managed.active_slot().as_str() == crate::target::PREVIEW_SLOT
        && !snapshot.preview().is_some_and(|preview| {
            preview.slot_id() == managed.active_slot()
                && preview.inodes() == managed.active_inodes()
        })
    {
        return Err(ServiceError::RecoveryRequired);
    }
    Ok(())
}

fn shares_base_inode(package: &ManagedPackage, target: &SlotView) -> bool {
    let base = package.base_inodes();
    let preview = target.inodes();
    let base_inodes = [base.ce(), base.de()];
    base_inodes.contains(&preview.ce()) || base_inodes.contains(&preview.de())
}
