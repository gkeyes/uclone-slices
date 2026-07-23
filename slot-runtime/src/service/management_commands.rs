use crate::domain::{PackageName, SlotId};
use crate::protocol::{
    Ack, AckOperation, ManagedAppSummary, ManagedAppsReport, PackageInspectionReport,
    ReconcileReport, RecoveryTargetsReport, ResponsePayload, SlotSummary, SlotsReport,
};
use crate::reconcile::ReconcileOutcome;
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

    pub(super) fn recovery_targets_command(&self) -> Result<ResponsePayload, ServiceError> {
        let targets = self.platform.list_recovery_targets()?;
        if targets.len() > super::MAX_RECOVERY_TARGETS {
            return Err(ServiceError::Internal);
        }
        Ok(ResponsePayload::RecoveryTargets(
            RecoveryTargetsReport::new(targets),
        ))
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
        let snapshot = validation::snapshot(&key, self.platform.package_state(&key)?)?;
        validation::switchable(snapshot.managed())?;
        let enrollment = snapshot.managed().clone();
        match self
            .platform
            .create_slot(&key, display_name.clone(), seed_mode)?
        {
            SwitchExecution::Committed(view) => {
                self.confirmed_switch_result(package, &enrollment, &view)
            }
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
        let mut aggregate = None;
        for package in packages {
            let key = validation::package_key(&package);
            let result = self.platform.reconcile_two_phase(&key);
            let failure = reconcile_failure(&result);
            aggregate = merge_reconcile_failure(aggregate, failure);
        }
        if let Some(error) = aggregate {
            return Err(error);
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

const fn reconcile_failure(
    result: &Result<ReconcileOutcome, ServiceError>,
) -> Option<ServiceError> {
    match result {
        Ok(ReconcileOutcome::Locked | ReconcileOutcome::Held) | Err(ServiceError::UserLocked) => {
            Some(ServiceError::UserLocked)
        }
        Ok(ReconcileOutcome::RecoveryRequired(_) | ReconcileOutcome::Quarantined)
        | Err(
            ServiceError::RecoveryRequired
            | ServiceError::Quarantined
            | ServiceError::NotFound
            | ServiceError::Conflict,
        ) => Some(ServiceError::RecoveryRequired),
        Ok(
            ReconcileOutcome::RestoredBase
            | ReconcileOutcome::RestoredSlot(_)
            | ReconcileOutcome::RolledBack
            | ReconcileOutcome::RolledForward,
        ) => None,
        Err(_) => Some(ServiceError::Internal),
    }
}

const fn merge_reconcile_failure(
    current: Option<ServiceError>,
    next: Option<ServiceError>,
) -> Option<ServiceError> {
    match (current, next) {
        (Some(ServiceError::Internal), _) | (_, Some(ServiceError::Internal)) => {
            Some(ServiceError::Internal)
        }
        (Some(ServiceError::RecoveryRequired), _) | (_, Some(ServiceError::RecoveryRequired)) => {
            Some(ServiceError::RecoveryRequired)
        }
        (Some(ServiceError::UserLocked), _) | (_, Some(ServiceError::UserLocked)) => {
            Some(ServiceError::UserLocked)
        }
        (None, None) => None,
        _ => Some(ServiceError::Internal),
    }
}
