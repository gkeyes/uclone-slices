use crate::android::PackageProbe;
use crate::domain::{ManagedPackage, PackageKey};
use crate::lifecycle::LifecycleState;
use crate::materializer::MaterializationBackend;
use crate::package_state::PackageStateReason;
use crate::reconcile::{ReconcileOutcome, RecoveryBackend};
use crate::service::ServiceError;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_contain(
        &mut self,
        package: &ManagedPackage,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.do_contain_exact(package)?;
        if class == ServiceError::Quarantined {
            self.mark_package_quarantined(package)
        } else {
            self.mark_package_recovery(package)
        }
    }

    pub(super) fn do_contain_exact(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), ServiceError> {
        if self.runtime.capture_gate_snapshot(package).is_err() {
            self.runtime
                .emergency_gate(package.package_name())
                .map_err(|_| ServiceError::RecoveryRequired)?;
        }
        self.runtime
            .acquire_gate(package)
            .and_then(|()| self.runtime.verify_gate_held(package))
            .and_then(|()| self.runtime.quiesce_processes(package))
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    pub(super) fn mark_package_recovery(
        &self,
        package: &ManagedPackage,
    ) -> Result<(), ServiceError> {
        self.transition_state(
            package,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
    }

    pub(super) fn mark_package_quarantined(
        &self,
        package: &ManagedPackage,
    ) -> Result<(), ServiceError> {
        self.transition_state(
            package,
            LifecycleState::Quarantined,
            PackageStateReason::IdentityChanged,
        )
    }

    pub(super) fn map_reconcile_state(
        &self,
        package: &ManagedPackage,
        outcome: &ReconcileOutcome,
    ) -> Result<(), ServiceError> {
        match outcome {
            ReconcileOutcome::Quarantined => self.mark_package_quarantined(package),
            ReconcileOutcome::RecoveryRequired(_) => self.mark_package_recovery(package),
            ReconcileOutcome::RestoredBase
            | ReconcileOutcome::RestoredSlot(_)
            | ReconcileOutcome::RolledBack
            | ReconcileOutcome::RolledForward => self.mark_package_reconciled(package),
            ReconcileOutcome::Held | ReconcileOutcome::Locked => Ok(()),
        }
    }

    fn mark_package_reconciled(&self, package: &ManagedPackage) -> Result<(), ServiceError> {
        let key = PackageKey::new(package.package_name().clone(), package.user_id());
        let current = self
            .stores
            .package_state
            .latest(&key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .ok_or(ServiceError::RecoveryRequired)?;
        let mut lifecycle = current.lifecycle_state();
        if lifecycle == LifecycleState::Normal {
            return Ok(());
        }
        if lifecycle == LifecycleState::RecoveryRequired {
            self.stores
                .package_state
                .transition(
                    &key,
                    lifecycle,
                    LifecycleState::RepairWaiting,
                    PackageStateReason::ManualRepair,
                )
                .map_err(|_| ServiceError::RecoveryRequired)?;
            lifecycle = LifecycleState::RepairWaiting;
        }
        if lifecycle != LifecycleState::RepairWaiting {
            return Err(ServiceError::RecoveryRequired);
        }
        self.stores
            .package_state
            .transition(
                &key,
                lifecycle,
                LifecycleState::Normal,
                PackageStateReason::ManualRepair,
            )
            .map(|_| ())
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn transition_state(
        &self,
        package: &ManagedPackage,
        target: LifecycleState,
        reason: PackageStateReason,
    ) -> Result<(), ServiceError> {
        let key = PackageKey::new(package.package_name().clone(), package.user_id());
        let current = self
            .stores
            .package_state
            .latest(&key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .ok_or(ServiceError::RecoveryRequired)?;
        if current.lifecycle_state() == target {
            return Ok(());
        }
        self.stores
            .package_state
            .transition(&key, current.lifecycle_state(), target, reason)
            .map(|_| ())
            .map_err(|_| ServiceError::RecoveryRequired)
    }
}
