use crate::android::PackageProbe;
use crate::domain::{PackageKey, PackageSupportLevel};
use crate::materializer::MaterializationBackend;
use crate::reconcile::{ReconcileOutcome, ReconcileReason, RecoveryBackend};
use crate::service::ServiceError;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;
use super::reconciliation::ReleasePolicy;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn release_policy(&self, key: &PackageKey) -> Result<ReleasePolicy, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        if !probe
            .user0_unlocked()
            .map_err(|_| ServiceError::RecoveryRequired)?
        {
            return Ok(ReleasePolicy::UserLocked);
        }
        let policy = self
            .stores
            .compatibility_policy
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let Some(policy) = policy else {
            return Ok(ReleasePolicy::Invalid);
        };
        let candidate = probe
            .inspect_package(key.package_name(), key.user_id())
            .map_err(|_| ServiceError::RecoveryRequired)?;
        if candidate.pending_install() {
            return Ok(ReleasePolicy::Invalid);
        }
        let level = candidate.compatibility().support_level();
        if !policy.accepts(candidate.identity(), level) {
            return Ok(ReleasePolicy::Invalid);
        }
        if level == PackageSupportLevel::DirectBootConditional {
            let active_non_base = self
                .stores
                .registry
                .latest(key.package_name())
                .map_err(|_| ServiceError::RecoveryRequired)?
                .is_some_and(|revision| !revision.active_slot().is_base());
            if active_non_base {
                return Ok(ReleasePolicy::ConditionalActiveSlot);
            }
        }
        Ok(ReleasePolicy::Allowed)
    }

    pub(super) fn retain_invalid_policy(
        &self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        let outcome = ReconcileOutcome::Quarantined;
        if let Some(package) = self.load_enrollment(key)? {
            self.map_reconcile_state(&package, &outcome)?;
        }
        Ok(outcome)
    }

    pub(super) fn retain_conditional_boot(
        &self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        let outcome = ReconcileOutcome::RecoveryRequired(ReconcileReason::ConditionalDirectBoot);
        if let Some(package) = self.load_enrollment(key)? {
            self.map_reconcile_state(&package, &outcome)?;
        }
        Ok(outcome)
    }
}
