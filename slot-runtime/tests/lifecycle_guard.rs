#![doc = "Package lifecycle guard behavior tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, PackageObservation, SlotId,
    SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::{
    GuardDecision, LifecycleState, PackageLifecycleGuard, RecoveryReason,
};

const SIGNATURE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SIGNATURE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn managed(active_slot: SlotId, active_inodes: DataInodes) -> ManagedPackage {
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(10_321, SIGNATURE_A, 1, "/data/app/slotprobe/base.apk").unwrap(),
        DataInodes::new(100, 200).unwrap(),
        SlotView::new(active_slot, active_inodes),
        LifecycleState::Normal,
    )
    .unwrap()
}

const fn observation(
    identity: AppIdentity,
    package_manager: DataInodes,
    visible: DataInodes,
    pending_install: bool,
) -> PackageObservation {
    PackageObservation::new(identity, package_manager, visible, visible, pending_install)
}

#[test]
fn allows_base_when_identity_and_all_inodes_match() {
    let base = DataInodes::new(100, 200).unwrap();
    let managed = managed(SlotId::base(), base);
    let observed = observation(managed.identity().clone(), base, base, false);

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(decision, GuardDecision::AllowBase);
}

#[test]
fn allows_non_base_slot_only_while_package_manager_remains_anchored_to_base() {
    let slot = DataInodes::new(300, 400).unwrap();
    let managed = managed(SlotId::parse("work").unwrap(), slot);
    let observed = observation(
        managed.identity().clone(),
        DataInodes::new(100, 200).unwrap(),
        slot,
        false,
    );

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(decision, GuardDecision::AllowSlot);
}

#[test]
fn fails_closed_when_package_manager_inode_drifted_to_active_slot() {
    let slot = DataInodes::new(300, 400).unwrap();
    let managed = managed(SlotId::parse("work").unwrap(), slot);
    let observed = observation(managed.identity().clone(), slot, slot, false);

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(
        decision,
        GuardDecision::RecoveryRequired(RecoveryReason::PackageManagerInodeDrift),
    );
}

#[test]
fn quarantines_signature_change() {
    let base = DataInodes::new(100, 200).unwrap();
    let managed = managed(SlotId::base(), base);
    let changed = AppIdentity::new(10_321, SIGNATURE_B, 2, "/data/app/slotprobe/base.apk").unwrap();
    let observed = observation(changed, base, base, false);

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(decision, GuardDecision::Quarantine);
}

#[test]
fn requires_safe_update_window_when_installer_session_is_pending() {
    let base = DataInodes::new(100, 200).unwrap();
    let managed = managed(SlotId::base(), base);
    let observed = observation(managed.identity().clone(), base, base, true);

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(decision, GuardDecision::RequireSafeUpdateWindow);
}

#[test]
fn accepts_version_change_only_during_update_verification() {
    let base = DataInodes::new(100, 200).unwrap();
    let managed =
        managed(SlotId::base(), base).with_lifecycle_state(LifecycleState::UpdateVerifying);
    let updated =
        AppIdentity::new(10_321, SIGNATURE_A, 2, "/data/app/slotprobe-v2/base.apk").unwrap();
    let observed = observation(updated, base, base, false);

    let decision = PackageLifecycleGuard::assess(&managed, &observed);

    assert_eq!(decision, GuardDecision::AllowUpdateVerification);
}
