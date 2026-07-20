use crate::android::PackageProbe;
use crate::domain::{GateSnapshot, ManagedPackage, SlotId, SlotView};
use crate::materializer::{MaterializationBackend, MaterializationCoordinator, NoFault};
use crate::reconcile::RecoveryBackend;
use crate::runtime::{RuntimeError, SwitchCoordinator, SwitchRequest};
use crate::service::{ServiceError, SwitchExecution};
use crate::slot_metadata::{SlotRecordState, SlotSeedMode};

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_materialize(
        &mut self,
        package: &ManagedPackage,
        slot: &SlotId,
        seed_mode: SlotSeedMode,
    ) -> Result<SlotView, ServiceError> {
        let mut faults = NoFault;
        let Ok(result) = MaterializationCoordinator::new(&mut self.materializer, &mut faults)
            .materialize_with_seed(package, slot, seed_mode)
        else {
            self.mark_package_recovery(package)?;
            return Err(ServiceError::RecoveryRequired);
        };
        let entry = self.stores.catalog.append_slot(
            result.package_key(),
            result.slot_id().clone(),
            result.inodes(),
            result.enrolled_identity(),
            result.created_version_code(),
            result.security_profile().clone(),
        );
        if let Ok(entry) = entry {
            Ok(SlotView::new(entry.slot_id().clone(), entry.inodes()))
        } else {
            let _ = MaterializationCoordinator::new(&mut self.materializer, &mut faults)
                .cleanup_interrupted(package, slot);
            self.mark_package_recovery(package)?;
            Err(ServiceError::RecoveryRequired)
        }
    }

    pub(super) fn do_switch(
        &mut self,
        package: &ManagedPackage,
        target: &SlotView,
        prepared_gate: Option<GateSnapshot>,
    ) -> Result<SwitchExecution, ServiceError> {
        if !target.slot_id().is_base() {
            let metadata = self
                .stores
                .slot_metadata
                .latest(package.package_name(), target.slot_id());
            match metadata {
                Ok(Some(record)) if record.state() == SlotRecordState::Ready => {}
                Ok(None) => {}
                Ok(Some(_)) => return Err(ServiceError::Conflict),
                Err(_) => {
                    self.do_contain(package, ServiceError::RecoveryRequired)?;
                    return Ok(SwitchExecution::RecoveryRequired);
                }
            }
        }
        if let Some(expected) = prepared_gate {
            let Ok(actual) = self.runtime.capture_gate_snapshot(package) else {
                self.do_contain(package, ServiceError::RecoveryRequired)?;
                return Ok(SwitchExecution::RecoveryRequired);
            };
            if actual != expected {
                self.do_contain(package, ServiceError::RecoveryRequired)?;
                return Ok(SwitchExecution::RecoveryRequired);
            }
        }
        let metadata = match self.metadata.next() {
            Ok(metadata) => metadata,
            Err(_) if prepared_gate.is_some() => {
                self.do_contain(package, ServiceError::RecoveryRequired)?;
                return Ok(SwitchExecution::RecoveryRequired);
            }
            Err(error) => return Err(error),
        };
        let request = SwitchRequest::new(package.clone(), target.clone(), metadata);
        let outcome = SwitchCoordinator::new(
            &mut self.runtime,
            self.stores.journal.clone(),
            self.stores.registry.clone(),
        )
        .switch(&request);
        match outcome {
            Ok(value) => {
                let execution = super::mapping::switch(value);
                if execution == SwitchExecution::RecoveryRequired {
                    self.mark_package_recovery(package)?;
                }
                if matches!(execution, SwitchExecution::Committed(_))
                    && !target.slot_id().is_base()
                    && let Ok(Some(record)) = self
                        .stores
                        .slot_metadata
                        .latest(package.package_name(), target.slot_id())
                    && self
                        .stores
                        .slot_metadata
                        .update(
                            package.package_name(),
                            target.slot_id(),
                            record.display_name().clone(),
                            SlotRecordState::Ready,
                            package.identity().version_code(),
                        )
                        .is_err()
                {
                    self.do_contain(package, ServiceError::RecoveryRequired)?;
                    return Ok(SwitchExecution::RecoveryRequired);
                }
                Ok(execution)
            }
            Err(error) => self.switch_error(package, prepared_gate.is_some(), &error),
        }
    }

    fn switch_error(
        &mut self,
        package: &ManagedPackage,
        prepared: bool,
        error: &RuntimeError,
    ) -> Result<SwitchExecution, ServiceError> {
        let class = super::mapping::runtime(error);
        if prepared
            || matches!(
                class,
                ServiceError::RecoveryRequired | ServiceError::Quarantined
            )
        {
            self.do_contain(package, class)?;
            return Ok(if class == ServiceError::Quarantined {
                SwitchExecution::Quarantined
            } else {
                SwitchExecution::RecoveryRequired
            });
        }
        Err(class)
    }
}
