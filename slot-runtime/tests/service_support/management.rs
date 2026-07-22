#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::needless_pass_by_ref_mut,
    reason = "fake methods intentionally mirror fallible mutable service-platform seams"
)]

use uclone_slot_runtime::domain::{PackageKey, SlotId, SlotView};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::service::{
    ManagedAppInfo, PackageInspection, PackageState, ServiceError, SlotInfo, SwitchExecution,
};
use uclone_slot_runtime::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

use super::fixtures::{assert_key, managed_base, preview_view};
use super::{Call, FakePlatform};

impl FakePlatform {
    pub(super) fn inspect_fake(&self, key: &PackageKey) -> Result<PackageInspection, ServiceError> {
        assert_key(key);
        self.record(Call::Inspect);
        let package = managed_base();
        Ok(PackageInspection::new(
            key.package_name().clone(),
            package.identity().clone(),
            package.base_inodes(),
            self.inspection_compatibility,
        ))
    }

    pub(super) fn list_managed_fake(&self) -> Result<Vec<ManagedAppInfo>, ServiceError> {
        self.record(Call::ListManaged);
        match self.state.borrow().clone() {
            PackageState::Absent => Ok(Vec::new()),
            PackageState::Ready(snapshot) => Ok(vec![ManagedAppInfo::new(
                snapshot.managed().package_name().clone(),
                snapshot.managed().active_slot().clone(),
                snapshot.managed().lifecycle_state(),
            )]),
            PackageState::RecoveryRequired => Ok(vec![ManagedAppInfo::new(
                managed_base().package_name().clone(),
                SlotId::base(),
                LifecycleState::RecoveryRequired,
            )]),
            PackageState::Quarantined => Ok(vec![ManagedAppInfo::new(
                managed_base().package_name().clone(),
                SlotId::base(),
                LifecycleState::Quarantined,
            )]),
        }
    }

    pub(super) fn list_slots_fake(&self, key: &PackageKey) -> Result<Vec<SlotInfo>, ServiceError> {
        assert_key(key);
        self.record(Call::ListSlots);
        let PackageState::Ready(snapshot) = self.state.borrow().clone() else {
            return Err(ServiceError::NotFound);
        };
        let managed = snapshot.managed();
        let mut slots = vec![slot_info(
            SlotView::new(SlotId::base(), managed.base_inodes()),
            managed.active_slot().is_base(),
            "Base",
        )];
        slots.extend(snapshot.slots().iter().cloned().map(|view| {
            let active = managed.active_slot() == view.slot_id();
            slot_info(view, active, "Preview")
        }));
        Ok(slots)
    }

    pub(super) fn create_fake(
        &mut self,
        key: &PackageKey,
    ) -> Result<SwitchExecution, ServiceError> {
        assert_key(key);
        self.record(Call::CreateSlot);
        let target = preview_view();
        self.set_active(&target);
        Ok(SwitchExecution::Committed(target))
    }

    pub(super) fn rename_fake(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        assert_key(key);
        self.record(Call::RenameSlot);
        Ok(())
    }

    pub(super) fn delete_fake(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        assert_key(key);
        self.record(Call::DeleteSlot);
        Ok(())
    }
}

fn slot_info(view: SlotView, active: bool, name: &str) -> SlotInfo {
    SlotInfo::new(
        view.slot_id().clone(),
        SlotDisplayName::parse(name).unwrap(),
        SlotSeedMode::CloneBase,
        SlotRecordState::Ready,
        active,
        1,
        1,
        view.inodes(),
    )
}
