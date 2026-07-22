use crate::android::PackageProbe;
use crate::domain::{PackageKey, SlotId};
use crate::materializer::{MaterializationBackend, MaterializationCoordinator, NoFault};
use crate::reconcile::RecoveryBackend;
use crate::service::{ServiceError, SwitchExecution};
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
    pub(super) fn do_create_slot(
        &mut self,
        key: &PackageKey,
        display_name: SlotDisplayName,
        seed_mode: SlotSeedMode,
    ) -> Result<SwitchExecution, ServiceError> {
        let snapshot = self.ready_snapshot(key)?;
        let managed = snapshot.managed().clone();
        if !managed.active_slot().is_base() {
            return Err(ServiceError::Conflict);
        }
        let slot = self.metadata.next_slot_id()?;
        self.stores
            .slot_metadata
            .create(
                key.package_name(),
                slot.clone(),
                display_name,
                seed_mode,
                managed.identity().version_code(),
            )
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let gate = self.do_capture_gate(key)?;
        self.do_hold_gate(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.do_quiesce(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let target = self.do_materialize(&managed, &slot, seed_mode)?;
        self.do_switch(&managed, &target, Some(gate))
    }

    pub(super) fn do_rename_slot(
        &self,
        key: &PackageKey,
        slot: &SlotId,
        display_name: SlotDisplayName,
    ) -> Result<(), ServiceError> {
        let snapshot = self.ready_snapshot(key)?;
        if slot.is_base() || snapshot.slot(slot).is_none() {
            return Err(ServiceError::Conflict);
        }
        let record = self.ready_record_or_adopt_legacy(key, slot)?;
        if record.state() != SlotRecordState::Ready {
            return Err(ServiceError::Conflict);
        }
        self.stores
            .slot_metadata
            .update(
                key.package_name(),
                slot,
                display_name,
                SlotRecordState::Ready,
                snapshot.managed().identity().version_code(),
            )
            .map(|_| ())
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    pub(super) fn do_delete_slot(
        &mut self,
        key: &PackageKey,
        slot: &SlotId,
    ) -> Result<(), ServiceError> {
        let snapshot = self.ready_snapshot(key)?;
        let managed = snapshot.managed().clone();
        if slot.is_base() || !managed.active_slot().is_base() || snapshot.slot(slot).is_none() {
            return Err(ServiceError::Conflict);
        }
        let record = self.ready_record_or_adopt_legacy(key, slot)?;
        if record.state() != SlotRecordState::Ready {
            return Err(ServiceError::Conflict);
        }
        let gate = self.do_capture_gate(key)?;
        self.do_hold_gate(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.do_quiesce(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let mut faults = NoFault;
        MaterializationCoordinator::new(&mut self.materializer, &mut faults)
            .cleanup_interrupted(&managed, slot)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.stores
            .slot_metadata
            .update(
                key.package_name(),
                slot,
                record.display_name().clone(),
                SlotRecordState::Deleted,
                managed.identity().version_code(),
            )
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.runtime
            .verify_native_base(&managed)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.runtime
            .restore_gate(&managed, gate)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.runtime
            .retire_gate_lease(&managed)
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn ready_record_or_adopt_legacy(
        &self,
        key: &PackageKey,
        slot: &SlotId,
    ) -> Result<SlotMetadata, ServiceError> {
        if let Some(record) = self
            .stores
            .slot_metadata
            .latest(key.package_name(), slot)
            .map_err(|_| ServiceError::RecoveryRequired)?
        {
            return Ok(record);
        }
        let catalog = self
            .stores
            .catalog
            .lookup(key, slot)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .ok_or(ServiceError::RecoveryRequired)?;
        let display_name =
            SlotDisplayName::parse(slot.as_str()).map_err(|_| ServiceError::RecoveryRequired)?;
        self.stores
            .slot_metadata
            .create(
                key.package_name(),
                slot.clone(),
                display_name.clone(),
                SlotSeedMode::CloneBase,
                catalog.created_version_code(),
            )
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.stores
            .slot_metadata
            .update(
                key.package_name(),
                slot,
                display_name,
                SlotRecordState::Ready,
                catalog.enrolled_identity().version_code(),
            )
            .map_err(|_| ServiceError::RecoveryRequired)
    }
}
