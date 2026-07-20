use crate::domain::{PackageName, SlotId};
use crate::protocol::{
    Ack, AckOperation, ManagedAppSummary, ManagedAppsReport, PackageInspectionReport,
    ReconcileReport, ResponsePayload, SlotSummary, SlotsReport, SwitchResult,
};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

use super::{PreviewService, ServiceError, ServicePlatform, SwitchExecution, outcome, validation};

impl<P: ServicePlatform> PreviewService<P> {
    pub(super) fn inspect_command(
        &self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        let report = self.platform.inspect_package(&key)?;
        Ok(ResponsePayload::PackageInspection(
            PackageInspectionReport::from(report),
        ))
    }

    pub(super) fn managed_apps_command(&self) -> Result<ResponsePayload, ServiceError> {
        let rows = self
            .platform
            .list_managed_apps()?
            .into_iter()
            .map(ManagedAppSummary::from)
            .collect();
        Ok(ResponsePayload::ManagedApps(ManagedAppsReport::new(rows)))
    }

    pub(super) fn slots_command(
        &self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        let rows = self
            .platform
            .list_slots(&key)?
            .into_iter()
            .map(SlotSummary::from)
            .collect();
        Ok(ResponsePayload::Slots(SlotsReport::new(
            package.clone(),
            rows,
        )))
    }

    pub(super) fn create_slot_command(
        &mut self,
        package: &PackageName,
        display_name: &SlotDisplayName,
        seed_mode: SlotSeedMode,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        match self
            .platform
            .create_slot(&key, display_name.clone(), seed_mode)?
        {
            SwitchExecution::Committed(view) => Ok(ResponsePayload::SwitchResult(
                SwitchResult::new(package.clone(), view.slot_id().clone()),
            )),
            SwitchExecution::RolledBack => Err(ServiceError::Conflict),
            SwitchExecution::RecoveryRequired => Err(ServiceError::RecoveryRequired),
            SwitchExecution::Quarantined => Err(ServiceError::Quarantined),
        }
    }

    pub(super) fn rename_slot_command(
        &mut self,
        package: &PackageName,
        slot: &SlotId,
        display_name: &SlotDisplayName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        self.platform
            .rename_slot(&key, slot, display_name.clone())?;
        Ok(ResponsePayload::Ack(Ack::new(AckOperation::RenameSlot)))
    }

    pub(super) fn delete_slot_command(
        &mut self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        self.platform.delete_slot(&key, slot)?;
        Ok(ResponsePayload::Ack(Ack::new(AckOperation::DeleteSlot)))
    }

    pub(super) fn reconcile_package_command(
        &mut self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        let reconciled = self.platform.reconcile_two_phase(&key)?;
        Ok(ResponsePayload::ReconcileReport(ReconcileReport::new(
            package.clone(),
            outcome::reconcile(reconciled),
        )))
    }

    pub(super) fn reconcile_all_command(&mut self) -> Result<ResponsePayload, ServiceError> {
        let packages: Vec<_> = self
            .platform
            .list_managed_apps()?
            .into_iter()
            .map(|row| row.package().clone())
            .collect();
        for package in packages {
            let key = validation::package_key(&package);
            let outcome = self.platform.reconcile_two_phase(&key)?;
            match outcome {
                crate::reconcile::ReconcileOutcome::Locked
                | crate::reconcile::ReconcileOutcome::Held => {
                    return Err(ServiceError::UserLocked);
                }
                crate::reconcile::ReconcileOutcome::RecoveryRequired(_)
                | crate::reconcile::ReconcileOutcome::Quarantined => {
                    return Err(ServiceError::RecoveryRequired);
                }
                crate::reconcile::ReconcileOutcome::RestoredBase
                | crate::reconcile::ReconcileOutcome::RestoredSlot(_)
                | crate::reconcile::ReconcileOutcome::RolledBack
                | crate::reconcile::ReconcileOutcome::RolledForward => {}
            }
        }
        Ok(ResponsePayload::Ack(Ack::new(AckOperation::ReconcileAll)))
    }

    pub(super) fn retire_package_command(
        &mut self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        match self.platform.rescue_to_base(&key)? {
            crate::rescue::RescueExecution::CompletedBase => {
                Ok(ResponsePayload::Ack(Ack::new(AckOperation::RetirePackage)))
            }
            crate::rescue::RescueExecution::RecoveryRequired => Err(ServiceError::RecoveryRequired),
            crate::rescue::RescueExecution::Quarantined => Err(ServiceError::Quarantined),
            crate::rescue::RescueExecution::ContainmentFailed => Err(ServiceError::Internal),
        }
    }
}
