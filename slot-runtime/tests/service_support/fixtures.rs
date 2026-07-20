use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::service::{ObservedGateState, PackageSnapshot, PackageState};

pub(crate) fn allowed() -> PackageName {
    PackageName::parse("com.uclone.slotprobe").unwrap()
}

pub(crate) fn managed_base() -> ManagedPackage {
    ManagedPackage::new(
        PackageKey::new(allowed(), UserId::PRIMARY),
        identity(),
        base_inodes(),
        SlotView::new(SlotId::base(), base_inodes()),
        LifecycleState::Normal,
    )
    .unwrap()
}

pub(crate) fn ready_base() -> PackageState {
    PackageState::Ready(Box::new(PackageSnapshot::new(
        managed_base(),
        None,
        ObservedGateState::new(true, false),
    )))
}

pub(crate) fn preview_view() -> SlotView {
    SlotView::new(
        SlotId::parse("preview").unwrap(),
        DataInodes::new(301, 401).unwrap(),
    )
}

pub(crate) fn request(
    command: uclone_slot_runtime::protocol::Command,
) -> uclone_slot_runtime::protocol::Request {
    uclone_slot_runtime::protocol::Request::new(
        uclone_slot_runtime::protocol::RequestId::new("service-test").unwrap(),
        command,
    )
    .unwrap()
}

pub(crate) fn base_inodes() -> DataInodes {
    DataInodes::new(101, 201).unwrap()
}

pub(crate) const fn original_gate() -> GateSnapshot {
    GateSnapshot::new(PackageEnabledState::Default, false)
}

pub(crate) fn assert_key(key: &PackageKey) {
    assert_eq!(key.package_name(), &allowed());
    assert_eq!(key.user_id(), UserId::PRIMARY);
}

fn identity() -> AppIdentity {
    AppIdentity::new(10_321, &"ab".repeat(32), 1, "/data/app/slotprobe/base.apk").unwrap()
}
