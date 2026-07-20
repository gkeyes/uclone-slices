use crate::android::PackageProbe;
use crate::domain::{ManagedPackage, PackageKey};
use crate::enrollment_attempt::{EnrollmentAttempt, EnrollmentAttemptPhase, RetirementProof};
use crate::journal::Transaction;
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
        let report = {
            let mut reconciler = Reconciler::new(
                &mut self.runtime,
                self.stores.enrollment.clone(),
                self.stores.journal.clone(),
                self.stores.registry.clone(),
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
        let report = Reconciler::new(
            &mut self.runtime,
            self.stores.enrollment.clone(),
            self.stores.journal.clone(),
            self.stores.registry.clone(),
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

    fn pristine_pending(&self, key: &PackageKey) -> Result<bool, ServiceError> {
        let no_enrollment = self
            .stores
            .enrollment
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_catalog = self
            .stores
            .catalog
            .list(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_empty();
        let no_state = self
            .stores
            .package_state
            .latest(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_registry = self
            .stores
            .registry
            .latest(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_journal = !self
            .stores
            .journal
            .list()
            .map_err(|_| ServiceError::RecoveryRequired)?
            .iter()
            .any(|transaction| belongs_to(transaction, key));
        Ok(no_enrollment && no_catalog && no_state && no_registry && no_journal)
    }

    fn load_enrollment(&self, key: &PackageKey) -> Result<Option<ManagedPackage>, ServiceError> {
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

fn belongs_to(transaction: &Transaction, key: &PackageKey) -> bool {
    transaction.spec().package_name() == key.package_name()
        && transaction.spec().user_id() == key.user_id()
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
