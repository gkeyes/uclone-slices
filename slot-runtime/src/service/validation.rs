use crate::domain::{ManagedPackage, PackageKey, PackageName, SlotView, UserId};
use crate::lifecycle::LifecycleState;

use super::{PackageSnapshot, PackageState, ServiceError};

pub(super) fn package_key(package: &PackageName) -> PackageKey {
    PackageKey::new(package.clone(), UserId::PRIMARY)
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

pub(super) fn preview_target(
    package: &ManagedPackage,
    target: &SlotView,
) -> Result<(), ServiceError> {
    if target.slot_id().is_base() || shares_base_inode(package, target) {
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
    for slot in snapshot.slots() {
        preview_target(managed, slot)?;
    }
    if !managed.active_slot().is_base()
        && !snapshot
            .slot(managed.active_slot())
            .is_some_and(|slot| slot.inodes() == managed.active_inodes())
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
