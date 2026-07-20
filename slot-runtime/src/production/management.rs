use crate::android::PackageProbe;
use crate::domain::{PackageKey, SlotId};
use crate::lifecycle::LifecycleState;
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::{ManagedAppInfo, PackageInspection, PackageState, ServiceError, SlotInfo};
use crate::slot_metadata::{SlotDisplayName, SlotMetadata, SlotRecordState, SlotSeedMode};

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_inspect_package(
        &self,
        key: &PackageKey,
    ) -> Result<PackageInspection, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        let (package, compatibility) = super::enrollment::inspect_candidate(&mut *probe, key)?;
        Ok(PackageInspection::new(
            key.package_name().clone(),
            package.identity().clone(),
            package.base_inodes(),
            compatibility,
        ))
    }

    pub(super) fn do_list_managed(&self) -> Result<Vec<ManagedAppInfo>, ServiceError> {
        let enrolled = self
            .stores
            .enrollment
            .list()
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        enrolled
            .into_iter()
            .map(|package| {
                let key = PackageKey::new(package.package_name().clone(), package.user_id());
                let (active, lifecycle) = match super::state::load(&self.stores, &mut *probe, &key)?
                {
                    PackageState::Ready(snapshot) => (
                        snapshot.managed().active_slot().clone(),
                        snapshot.managed().lifecycle_state(),
                    ),
                    PackageState::RecoveryRequired => {
                        (SlotId::base(), LifecycleState::RecoveryRequired)
                    }
                    PackageState::Quarantined => (SlotId::base(), LifecycleState::Quarantined),
                    PackageState::Absent => return Err(ServiceError::RecoveryRequired),
                };
                Ok(ManagedAppInfo::new(
                    package.package_name().clone(),
                    active,
                    lifecycle,
                ))
            })
            .collect()
    }

    pub(super) fn do_list_slots(&self, key: &PackageKey) -> Result<Vec<SlotInfo>, ServiceError> {
        let snapshot = self.ready_snapshot(key)?;
        let managed = snapshot.managed();
        let metadata = self
            .stores
            .slot_metadata
            .list(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let mut result = vec![SlotInfo::new(
            SlotId::base(),
            SlotDisplayName::parse("Base").map_err(|_| ServiceError::Internal)?,
            SlotSeedMode::CloneBase,
            SlotRecordState::Ready,
            managed.active_slot().is_base(),
            managed.identity().version_code(),
            managed.identity().version_code(),
            managed.base_inodes(),
        )];
        for view in snapshot.slots() {
            let record = metadata.iter().find(|value| value.slot() == view.slot_id());
            if record.is_some_and(|value| value.state() == SlotRecordState::Deleted) {
                continue;
            }
            if record.is_some_and(|value| value.state() != SlotRecordState::Ready) {
                return Err(ServiceError::RecoveryRequired);
            }
            let display = record
                .map_or_else(
                    || SlotDisplayName::parse(view.slot_id().as_str()),
                    |value| Ok(value.display_name().clone()),
                )
                .map_err(|_| ServiceError::RecoveryRequired)?;
            result.push(SlotInfo::new(
                view.slot_id().clone(),
                display,
                record.map_or(SlotSeedMode::CloneBase, SlotMetadata::seed_mode),
                SlotRecordState::Ready,
                managed.active_slot() == view.slot_id(),
                record.map_or_else(
                    || managed.identity().version_code(),
                    SlotMetadata::created_version_code,
                ),
                record.map_or_else(
                    || managed.identity().version_code(),
                    SlotMetadata::last_opened_version_code,
                ),
                view.inodes(),
            ));
        }
        if metadata.iter().any(|value| {
            value.state() != SlotRecordState::Deleted
                && !snapshot
                    .slots()
                    .iter()
                    .any(|view| view.slot_id() == value.slot())
        }) {
            return Err(ServiceError::RecoveryRequired);
        }
        Ok(result)
    }

    pub(super) fn ready_snapshot(
        &self,
        key: &PackageKey,
    ) -> Result<Box<crate::service::PackageSnapshot>, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        match super::state::load(&self.stores, &mut *probe, key)? {
            PackageState::Ready(snapshot) => Ok(snapshot),
            PackageState::Absent => Err(ServiceError::NotFound),
            PackageState::RecoveryRequired => Err(ServiceError::RecoveryRequired),
            PackageState::Quarantined => Err(ServiceError::Quarantined),
        }
    }
}
