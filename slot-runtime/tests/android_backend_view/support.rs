use uclone_slot_runtime::android::{CanonicalView, MountCounts, ViewProof};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;

pub(super) fn managed(active_view: Option<SlotView>) -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        base,
        active_view.unwrap_or_else(|| SlotView::new(SlotId::base(), base)),
        LifecycleState::Normal,
    )
    .unwrap()
}

pub(super) const fn held() -> GateSnapshot {
    GateSnapshot::new(PackageEnabledState::DisabledUser, false)
}

pub(super) const fn coherent(inodes: DataInodes, counts: MountCounts) -> ViewProof {
    ViewProof::new(CanonicalView::new(inodes, counts), inodes, inodes)
}
