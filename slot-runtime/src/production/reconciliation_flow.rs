use crate::android::PackageProbe;
use crate::domain::PackageKey;
use crate::materializer::MaterializationBackend;
use crate::reconcile::{
    NativeBaseRecoveryBackend, ReconcileOutcome, ReconcileReason, RecoveryBackend,
};
use crate::rescue::{RescueMetadataSource, RescueStartup};
use crate::service::ServiceError;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource + RescueMetadataSource,
{
    pub(super) fn reconcile_ordinary(
        &mut self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        match self.do_reconcile(key) {
            Ok(outcome @ ReconcileOutcome::RestoredBase)
            | Ok(outcome @ ReconcileOutcome::RestoredSlot(_))
            | Ok(outcome @ ReconcileOutcome::RolledBack)
            | Ok(outcome @ ReconcileOutcome::RolledForward) => {
                self.clear_recovery_override(key);
                Ok(outcome)
            }
            Ok(outcome @ ReconcileOutcome::RecoveryRequired(_))
            | Ok(outcome @ ReconcileOutcome::Quarantined) => {
                let class = if matches!(&outcome, ReconcileOutcome::Quarantined) {
                    ServiceError::Quarantined
                } else {
                    ServiceError::RecoveryRequired
                };
                if self.contain_reconcile_failure(key, class) {
                    Ok(outcome)
                } else {
                    Err(ServiceError::Internal)
                }
            }
            Ok(outcome) => Ok(outcome),
            Err(_) => {
                if self.contain_reconcile_failure(key, ServiceError::RecoveryRequired) {
                    Ok(ReconcileOutcome::RecoveryRequired(
                        ReconcileReason::JournalMetadata,
                    ))
                } else {
                    Err(ServiceError::Internal)
                }
            }
        }
    }

    pub(super) fn reconcile_with_rescue(
        &mut self,
        key: &PackageKey,
    ) -> Result<ReconcileOutcome, ServiceError> {
        match self.do_rescue_startup(key) {
            RescueStartup::OpenOrdinary => self.reconcile_ordinary(key),
            RescueStartup::BaseRetired => {
                self.clear_recovery_override(key);
                Ok(ReconcileOutcome::RestoredBase)
            }
            RescueStartup::RecoveryRequired => {
                if self.contain_reconcile_failure(key, ServiceError::RecoveryRequired) {
                    Ok(ReconcileOutcome::RecoveryRequired(
                        ReconcileReason::JournalMetadata,
                    ))
                } else {
                    Err(ServiceError::Internal)
                }
            }
            RescueStartup::Quarantined => {
                if self.contain_reconcile_failure(key, ServiceError::Quarantined) {
                    Ok(ReconcileOutcome::Quarantined)
                } else {
                    Err(ServiceError::Internal)
                }
            }
            RescueStartup::ContainmentFailed => {
                self.add_recovery_override(key);
                Err(ServiceError::Internal)
            }
        }
    }
}
