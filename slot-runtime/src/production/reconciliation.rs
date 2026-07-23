use crate::android::PackageProbe;
use crate::domain::{ManagedPackage, PackageKey};
use crate::enrollment_attempt::{EnrollmentAttempt, EnrollmentAttemptPhase, RetirementProof};
use crate::materializer::MaterializationBackend;
use crate::reconcile::{ReconcileOutcome, ReconcileReason, Reconciler, RecoveryBackend};
use crate::service::{PackageState, ServiceError};

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn contain_reconcile_failure(
        &mut self,
        key: &PackageKey,
        class: ServiceError,
    ) -> bool {
        self.add_recovery_override(key);
        if self.runtime.emergency_gate(key.package_name()).is_err() {
            return false;
        }
        let Ok(Some(package)) = self.stores.enrollment.load(key.package_name()) else {
            return true;
        };
        let _ = if class == ServiceError::Quarantined {
            self.mark_package_quarantined(&package)
        } else {
            self.mark_package_recovery(&package)
        };
        if self.runtime.confirm_emergency_gate(&package).is_err() {
            return false;
        }
        self.do_contain_exact(&package).is_ok()
    }

    pub(super) fn do_reconcile(
        &mut self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        super::enrollment::require_key(key)?;
        let early = self.hold_early_boot(key)?;
        let attempt = self
            .stores
            .attempts
            .load(key)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        let Some(early) = early else {
            if attempt.is_none() && self.is_clean_unenrolled(key)? {
                return match self.runtime.emergency_gate_if_leased(key.package_name()) {
                    Ok(Some(_)) => self.abort_pristine_orphan(key),
                    Ok(None) => Ok(ReconcileOutcome::Held),
                    Err(_) => Ok(enrollment_recovery()),
                };
            }
            return Err(ServiceError::RecoveryRequired);
        };
        if let Some(attempt) = attempt.as_ref() {
            match attempt.phase() {
                EnrollmentAttemptPhase::Pending if self.pristine_pending(key)? => {
                    return self.abort_pristine_pending(key, attempt);
                }
                EnrollmentAttemptPhase::Committed
                    if attempt.is_authoritative()
                        && super::reconciliation_validation::publications_match(
                            &self.stores,
                            key,
                            attempt,
                        )? => {}
                EnrollmentAttemptPhase::Pending
                | EnrollmentAttemptPhase::RecoveryRequired
                | EnrollmentAttemptPhase::Committed => {
                    self.retain_attempt_uncertainty(key);
                    return Ok(enrollment_recovery());
                }
            }
        }
        if early == enrollment_recovery() && attempt.is_none() {
            if self.pristine_pending(key)? {
                return self.abort_pristine_orphan(key);
            }
            return Ok(early);
        }
        if attempt.is_none()
            && let Some(outcome) = self.cleanup_unpublished_preview(key)?
        {
            return Ok(outcome);
        }
        match self.release_policy(key)? {
            ReleasePolicy::Allowed => {}
            ReleasePolicy::UserLocked => return Ok(ReconcileOutcome::Locked),
            ReleasePolicy::ConditionalActiveSlot => {
                return self.retain_conditional_boot(key);
            }
            ReleasePolicy::Invalid => return self.retain_invalid_policy(key),
        }
        let report = {
            let mut reconciler = Reconciler::for_package(
                &mut self.runtime,
                self.stores.enrollment.clone(),
                self.stores.journal.clone(),
                self.stores.registry.clone(),
                key.package_name().clone(),
            );
            reconciler
                .early_boot()
                .and_then(|_| reconciler.reconcile_unlocked())
                .map_err(|_| ServiceError::RecoveryRequired)?
        };
        let outcome = report
            .results()
            .iter()
            .find(|result| result.package_name() == key.package_name())
            .map(|result| result.outcome().clone())
            .ok_or(ServiceError::NotFound)?;
        let managed = self.load_enrollment(key)?;
        if let Some(package) = managed.as_ref() {
            self.map_reconcile_state(package, &outcome)?;
        }
        if releases_gate(&outcome) {
            self.retire_reconciled_attempt(key, managed.as_ref(), attempt.as_ref())?;
        }
        Ok(outcome)
    }

    fn hold_early_boot(
        &mut self,
        key: &PackageKey,
    ) -> Result<Option<ReconcileOutcome>, ServiceError> {
        let report = Reconciler::for_package(
            &mut self.runtime,
            self.stores.enrollment.clone(),
            self.stores.journal.clone(),
            self.stores.registry.clone(),
            key.package_name().clone(),
        )
        .early_boot();
        let Ok(report) = report else {
            let _ = self.runtime.emergency_gate(key.package_name());
            return Err(ServiceError::RecoveryRequired);
        };
        Ok(report
            .results()
            .iter()
            .find(|result| result.package_name() == key.package_name())
            .map(|result| result.outcome().clone()))
    }

    fn is_clean_unenrolled(&self, key: &PackageKey) -> Result<bool, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        Ok(matches!(
            super::state::load(&self.stores, &mut *probe, key)?,
            PackageState::Absent
        ))
    }

    pub(super) fn load_enrollment(
        &self,
        key: &PackageKey,
    ) -> Result<Option<ManagedPackage>, ServiceError> {
        self.stores
            .enrollment
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn retire_reconciled_attempt(
        &mut self,
        key: &PackageKey,
        managed: Option<&ManagedPackage>,
        attempt: Option<&EnrollmentAttempt>,
    ) -> Result<(), ServiceError> {
        let Some(attempt) = attempt else {
            return Ok(());
        };
        if self
            .stores
            .attempts
            .retire(key, RetirementProof::new(attempt.gate_snapshot()))
            .is_ok()
        {
            return Ok(());
        }
        if let Some(package) = managed {
            self.do_contain(package, ServiceError::RecoveryRequired)?;
        }
        Err(ServiceError::RecoveryRequired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReleasePolicy {
    Allowed,
    UserLocked,
    ConditionalActiveSlot,
    Invalid,
}

const fn enrollment_recovery() -> ReconcileOutcome {
    ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata)
}

const fn releases_gate(outcome: &ReconcileOutcome) -> bool {
    matches!(
        outcome,
        ReconcileOutcome::RestoredBase
            | ReconcileOutcome::RestoredSlot(_)
            | ReconcileOutcome::RolledBack
            | ReconcileOutcome::RolledForward
    )
}
