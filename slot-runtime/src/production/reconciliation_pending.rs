use crate::android::PackageProbe;
use crate::domain::PackageKey;
use crate::enrollment_attempt::{EnrollmentAttempt, RetirementProof};
use crate::materializer::MaterializationBackend;
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
    pub(super) fn abort_pristine_orphan(
        &mut self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        if !self
            .runtime
            .user0_unlocked()
            .map_err(|_| ServiceError::RecoveryRequired)?
        {
            return Ok(ReconcileOutcome::Locked);
        }
        let snapshot = match self.runtime.emergency_gate_if_leased(key.package_name()) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                return Ok(ReconcileOutcome::RecoveryRequired(
                    ReconcileReason::MissingGateSnapshot,
                ));
            }
            Err(_) => {
                return Ok(self.pristine_failure(key, ReconcileReason::GateHoldFailed));
            }
        };
        let package = {
            let mut probe = self
                .probe
                .try_borrow_mut()
                .map_err(|_| ServiceError::Busy)?;
            super::enrollment::candidate_managed(&mut *probe, key)
        };
        let Ok(package) = package else {
            return Ok(self.pristine_failure(key, ReconcileReason::PackageStateDrift));
        };
        if self.runtime.confirm_emergency_gate(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::GateHoldFailed));
        }
        if self.runtime.verify_native_base(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::ViewRestoreFailed));
        }
        if self.runtime.restore_gate(&package, snapshot).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::GateRestoreFailed));
        }
        if self.runtime.retire_gate_lease(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::GateLeaseRetirement));
        }
        Ok(ReconcileOutcome::RestoredBase)
    }

    pub(super) fn abort_pristine_pending(
        &mut self,
        key: &PackageKey,
        attempt: &EnrollmentAttempt,
    ) -> Result<ReconcileOutcome, ServiceError> {
        if !self
            .runtime
            .user0_unlocked()
            .map_err(|_| ServiceError::RecoveryRequired)?
        {
            return Ok(ReconcileOutcome::Locked);
        }
        let package = {
            let mut probe = self
                .probe
                .try_borrow_mut()
                .map_err(|_| ServiceError::Busy)?;
            super::enrollment::candidate_managed(&mut *probe, key)
        };
        let Ok(package) = package else {
            return Ok(self.pristine_failure(key, ReconcileReason::PackageStateDrift));
        };
        if self.runtime.confirm_emergency_gate(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::GateHoldFailed));
        }
        if self.runtime.verify_native_base(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::ViewRestoreFailed));
        }
        if self
            .runtime
            .restore_gate(&package, attempt.gate_snapshot())
            .is_err()
        {
            return Ok(self.pristine_failure(key, ReconcileReason::GateRestoreFailed));
        }
        if self.runtime.retire_gate_lease(&package).is_err() {
            return Ok(self.pristine_failure(key, ReconcileReason::GateLeaseRetirement));
        }
        if self
            .stores
            .attempts
            .abort_pending(key, RetirementProof::new(attempt.gate_snapshot()))
            .is_err()
        {
            return Ok(self.pristine_failure(key, ReconcileReason::EnrollmentMetadata));
        }
        Ok(ReconcileOutcome::RestoredBase)
    }

    fn pristine_failure(&mut self, key: &PackageKey, reason: ReconcileReason) -> ReconcileOutcome {
        let _ = self.runtime.emergency_gate(key.package_name());
        ReconcileOutcome::RecoveryRequired(reason)
    }
}
