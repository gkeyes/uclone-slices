use crate::android::PackageProbe;
use crate::domain::{PackageKey, SlotId};
use crate::lifecycle::{GuardDecision, LifecycleState, PackageLifecycleGuard};
use crate::materializer::{MaterializationBackend, MaterializationCoordinator, NoFault};
use crate::reconcile::{ReconcileOutcome, ReconcileReason, RecoveryBackend};
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
    pub(super) fn retain_attempt_uncertainty(&self, key: &PackageKey) {
        let _ = self.stores.attempts.mark_recovery_required(key);
        if let Ok(Some(package)) = self.stores.enrollment.load(key.package_name()) {
            let _ = self.mark_package_recovery(&package);
        }
    }

    pub(super) fn cleanup_unpublished_preview(
        &mut self,
        key: &PackageKey,
    ) -> Result<Option<ReconcileOutcome>, ServiceError> {
        if !self
            .runtime
            .user0_unlocked()
            .map_err(|_| ServiceError::RecoveryRequired)?
        {
            return Ok(Some(ReconcileOutcome::Locked));
        }
        let Some(package) = self
            .stores
            .enrollment
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
        else {
            return Ok(Some(enrollment_recovery()));
        };
        let state = self
            .stores
            .package_state
            .latest(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let catalog = self
            .stores
            .catalog
            .list(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let Some(state) = state else {
            return self.recovery(&package);
        };
        if state.package_key() != *key {
            return self.recovery(&package);
        }
        let lifecycle = state.lifecycle_state();
        if lifecycle == LifecycleState::Quarantined {
            return Ok(Some(ReconcileOutcome::Quarantined));
        }
        let Some((_, preview)) = super::state::catalog_views(&catalog, key, &package) else {
            return self.recovery(&package);
        };
        if preview.is_some()
            && matches!(
                lifecycle,
                LifecycleState::RecoveryRequired | LifecycleState::RepairWaiting
            )
        {
            return Ok(None);
        }
        if lifecycle != LifecycleState::Normal {
            return self.recovery(&package);
        }
        if preview.is_some() {
            return Ok(None);
        }
        if self.has_committed_history(key)? {
            return self.recovery(&package);
        }
        let observation = self
            .runtime
            .observe_package(&package)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        match PackageLifecycleGuard::assess(&package, &observation) {
            GuardDecision::AllowBase => {}
            GuardDecision::Quarantine => {
                self.mark_package_quarantined(&package)?;
                return Ok(Some(ReconcileOutcome::Quarantined));
            }
            GuardDecision::AllowSlot
            | GuardDecision::AllowUpdateVerification
            | GuardDecision::RequireSafeUpdateWindow
            | GuardDecision::RecoveryRequired(_) => return self.recovery(&package),
        }
        let preview =
            SlotId::parse(crate::target::PREVIEW_SLOT).map_err(|_| ServiceError::Internal)?;
        let mut faults = NoFault;
        if MaterializationCoordinator::new(&mut self.materializer, &mut faults)
            .cleanup_interrupted(&package, &preview)
            .is_err()
        {
            return self.recovery(&package);
        }
        Ok(None)
    }

    fn has_committed_history(&self, key: &PackageKey) -> Result<bool, ServiceError> {
        let registry = self
            .stores
            .registry
            .latest(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let journal = self
            .stores
            .journal
            .list()
            .map_err(|_| ServiceError::RecoveryRequired)?;
        Ok(registry.is_some()
            || journal.iter().any(|transaction| {
                transaction.spec().package_name() == key.package_name()
                    && transaction.spec().user_id() == key.user_id()
            }))
    }

    fn recovery(
        &self,
        package: &crate::domain::ManagedPackage,
    ) -> Result<Option<ReconcileOutcome>, ServiceError> {
        self.mark_package_recovery(package)?;
        Ok(Some(enrollment_recovery()))
    }
}

const fn enrollment_recovery() -> ReconcileOutcome {
    ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata)
}
