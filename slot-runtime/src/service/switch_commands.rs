use crate::domain::{ManagedPackage, PackageName, SlotId, SlotView};
use crate::launch::LaunchDisposition;
use crate::protocol::{LaunchResult, LaunchStatus, ResponsePayload, SwitchResult};

use super::outcome;
use super::validation;
use super::{PreviewService, ServiceError, ServicePlatform};

impl<P: ServicePlatform> PreviewService<P> {
    pub(super) fn switch_command(
        &mut self,
        package: &PackageName,
        requested: &SlotId,
    ) -> Result<ResponsePayload, ServiceError> {
        let view = self.switch_verified_view(package, requested)?;
        Ok(ResponsePayload::SwitchResult(SwitchResult::new(
            package.clone(),
            view.slot_id().clone(),
        )))
    }

    pub(super) fn launch_current_command(
        &mut self,
        package: &PackageName,
        expected_slot: &SlotId,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        let before = validation::snapshot(&key, self.platform.package_state(&key)?)?;
        validation::switchable(before.managed())?;
        let view = active_view(&before, expected_slot)?;
        let enrollment = before.managed().clone();
        self.platform
            .verify_current_view_for_launch(&enrollment, &view)?;
        let snapshot = self.confirm_launch_snapshot(&key, &enrollment, &view)?;
        let launch_status = if !snapshot.gate().enabled() || snapshot.gate().suspended() {
            LaunchStatus::GateBlocked
        } else {
            match self.platform.launch_package(snapshot.managed()) {
                LaunchDisposition::Launched => LaunchStatus::Launched,
                LaunchDisposition::EntryNotFound => LaunchStatus::EntryNotFound,
                LaunchDisposition::IdentityChanged => {
                    self.contain_launch_failure(&enrollment, ServiceError::Quarantined)?;
                    return Err(ServiceError::Quarantined);
                }
                LaunchDisposition::PackageStateChanged => {
                    self.contain_launch_failure(&enrollment, ServiceError::RecoveryRequired)?;
                    return Err(ServiceError::RecoveryRequired);
                }
                LaunchDisposition::Failed => LaunchStatus::Failed,
            }
        };
        Ok(ResponsePayload::LaunchResult(LaunchResult::new(
            package.clone(),
            view.slot_id().clone(),
            launch_status,
        )))
    }

    pub(super) fn confirmed_switch_result(
        &mut self,
        package: &PackageName,
        enrollment: &ManagedPackage,
        view: SlotView,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        self.confirm_launch_snapshot(&key, enrollment, &view)?;
        Ok(ResponsePayload::SwitchResult(SwitchResult::new(
            package.clone(),
            view.slot_id().clone(),
        )))
    }

    fn switch_verified_view(
        &mut self,
        package: &PackageName,
        requested: &SlotId,
    ) -> Result<SlotView, ServiceError> {
        let key = validation::package_key(package);
        let snapshot = validation::snapshot(&key, self.platform.package_state(&key)?)?;
        let managed = snapshot.managed();
        validation::switchable(managed)?;
        let target = if requested.is_base() {
            SlotView::new(SlotId::base(), managed.base_inodes())
        } else {
            snapshot
                .slot(requested)
                .cloned()
                .ok_or(ServiceError::NotFound)?
        };
        if !target.slot_id().is_base() {
            validation::preview_target(managed, &target)?;
        }
        if managed.active_slot() == requested {
            return Ok(target);
        }
        let execution = self.platform.switch_view(managed, &target, None)?;
        if matches!(&execution, super::SwitchExecution::Committed(proved) if proved != &target) {
            self.contain_launch_failure(managed, ServiceError::RecoveryRequired)?;
        }
        let proved = outcome::proved_switch(&target, execution)?;
        Ok(proved)
    }

    fn confirm_launch_snapshot(
        &mut self,
        key: &crate::domain::PackageKey,
        enrollment: &ManagedPackage,
        view: &SlotView,
    ) -> Result<super::PackageSnapshot, ServiceError> {
        let observed = self.platform.package_state(key).and_then(|state| {
            let snapshot = validation::snapshot(key, state)?;
            validation::switchable(snapshot.managed())?;
            Ok(snapshot)
        });
        match observed {
            Ok(snapshot)
                if same_enrollment_contract(snapshot.managed(), enrollment)
                    && snapshot.managed().active_slot() == view.slot_id()
                    && snapshot.managed().active_inodes() == view.inodes() =>
            {
                Ok(snapshot)
            }
            Err(ServiceError::Quarantined) => {
                self.contain_launch_failure(enrollment, ServiceError::Quarantined)?;
                Err(ServiceError::Quarantined)
            }
            Ok(_) | Err(_) => {
                self.contain_launch_failure(enrollment, ServiceError::RecoveryRequired)?;
                Err(ServiceError::RecoveryRequired)
            }
        }
    }

    fn contain_launch_failure(
        &mut self,
        package: &ManagedPackage,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.platform
            .contain_failure(package, class)
            .map_err(|_| ServiceError::RecoveryRequired)
    }
}

fn same_enrollment_contract(left: &ManagedPackage, right: &ManagedPackage) -> bool {
    left.package_name() == right.package_name()
        && left.user_id() == right.user_id()
        && left.identity() == right.identity()
        && left.base_inodes() == right.base_inodes()
        && left.lifecycle_state() == right.lifecycle_state()
}

fn active_view(
    snapshot: &super::PackageSnapshot,
    expected_slot: &SlotId,
) -> Result<SlotView, ServiceError> {
    let managed = snapshot.managed();
    if managed.active_slot() != expected_slot {
        return Err(ServiceError::Conflict);
    }
    if expected_slot.is_base() {
        return Ok(SlotView::new(SlotId::base(), managed.base_inodes()));
    }
    let target = snapshot
        .slot(expected_slot)
        .cloned()
        .ok_or(ServiceError::NotFound)?;
    validation::preview_target(managed, &target)?;
    Ok(target)
}
