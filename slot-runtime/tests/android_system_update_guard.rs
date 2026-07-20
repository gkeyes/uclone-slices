#![doc = "Persisted `PackageManager` inode guard coverage for an active preview slot."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, PackageObservation, SlotId,
    SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::{
    GuardDecision, LifecycleState, PackageLifecycleGuard, RecoveryReason,
};

fn managed_preview() -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    let preview = DataInodes::new(303, 404).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(
            12_345,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/com.uclone.slotprobe/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::parse("preview").unwrap(), preview),
        LifecycleState::Normal,
    )
    .unwrap()
}

#[test]
fn active_preview_allows_persisted_base_package_manager_inodes() {
    let managed = managed_preview();
    let observation = PackageObservation::new(
        managed.identity().clone(),
        managed.base_inodes(),
        managed.active_inodes(),
        managed.active_inodes(),
        false,
    );

    assert_eq!(
        PackageLifecycleGuard::assess(&managed, &observation),
        GuardDecision::AllowSlot
    );
}

#[test]
fn active_preview_rejects_package_manager_inode_drift_to_preview() {
    let managed = managed_preview();
    let observation = PackageObservation::new(
        managed.identity().clone(),
        managed.active_inodes(),
        managed.active_inodes(),
        managed.active_inodes(),
        false,
    );

    assert_eq!(
        PackageLifecycleGuard::assess(&managed, &observation),
        GuardDecision::RecoveryRequired(RecoveryReason::PackageManagerInodeDrift)
    );
}
