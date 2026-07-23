use crate::domain::SlotView;
use crate::journal::{JournalEvent, TransactionSpec};
use crate::registry::RegistryError;

use super::{REASON_JOURNAL_FAILURE, REASON_PLATFORM_FAILURE, SwitchCoordinator};
use crate::runtime::RuntimeError;
use crate::runtime::{PlatformError, RecoveryCause, RuntimeBackend, SwitchOutcome};

const REASON_REGISTRY_FAILURE: &str = "registry_failure";
const REASON_ROLLBACK_FAILURE: &str = "rollback_failure";

impl<B: RuntimeBackend> SwitchCoordinator<B> {
    pub(super) fn rollback_platform(
        &mut self,
        spec: &TransactionSpec,
        original: PlatformError,
    ) -> Result<SwitchOutcome, RuntimeError> {
        let transaction = match self.stores.journal().load(spec.transaction_id()) {
            Ok(transaction) => transaction,
            Err(error) => {
                return self.require_recovery_before_applying(
                    spec,
                    RecoveryCause::Journal(error),
                    REASON_JOURNAL_FAILURE,
                );
            }
        };
        if !transaction
            .steps()
            .iter()
            .any(|step| matches!(step.event(), JournalEvent::Applying))
        {
            return self.require_recovery_before_applying(
                spec,
                RecoveryCause::Platform(original),
                REASON_PLATFORM_FAILURE,
            );
        }
        if let Err(error) = self.append(spec, JournalEvent::RollingBack) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        let previous = SlotView::new(spec.previous_slot().clone(), spec.previous_inodes());
        let rollback = self
            .backend
            .apply_slot_view(spec.managed_package(), &previous)
            .and_then(|()| {
                self.backend
                    .verify_slot_view(spec.managed_package(), &previous)
            });
        if let Err(error) = rollback {
            return self.require_recovery(
                spec,
                RecoveryCause::RollbackFailed {
                    original,
                    rollback: error,
                },
                REASON_ROLLBACK_FAILURE,
            );
        }
        if let Err(error) = self.append(spec, JournalEvent::RolledBack) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        let Some(release_package) = super::package_for_view(spec, &previous) else {
            return self.require_recovery(
                spec,
                RecoveryCause::Platform(PlatformError::view_verification(
                    "previous release contract invalid",
                )),
                REASON_ROLLBACK_FAILURE,
            );
        };
        if let Err(error) = self
            .backend
            .restore_gate(&release_package, spec.gate_snapshot())
        {
            return self.require_recovery(
                spec,
                RecoveryCause::RollbackFailed {
                    original,
                    rollback: error,
                },
                REASON_ROLLBACK_FAILURE,
            );
        }
        if let Err(error) = self.append(spec, JournalEvent::GateReleased) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults
            .check(crate::runtime::FaultPoint::GateReleased)?;
        if let Err(error) = self.append(spec, JournalEvent::Completed) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(crate::runtime::FaultPoint::Completed)?;
        if let Some(outcome) = self.retire_completed_lease(spec)? {
            return Ok(outcome);
        }
        self.faults
            .check(crate::runtime::FaultPoint::GateLeaseRetired)?;
        Ok(SwitchOutcome::RolledBack { cause: original })
    }

    pub(super) fn require_recovery_before_applying(
        &mut self,
        spec: &TransactionSpec,
        cause: RecoveryCause,
        reason: &str,
    ) -> Result<SwitchOutcome, RuntimeError> {
        if let Err(containment) = self
            .backend
            .acquire_gate(spec.managed_package())
            .and_then(|()| self.backend.verify_gate_held(spec.managed_package()))
        {
            return Err(RuntimeError::ContainmentFailed { cause, containment });
        }
        self.persist_recovery_required(spec, cause, reason)
    }

    pub(super) fn registry_recovery(
        &mut self,
        spec: &TransactionSpec,
        error: RegistryError,
    ) -> Result<SwitchOutcome, RuntimeError> {
        self.require_recovery(
            spec,
            RecoveryCause::Registry(error),
            REASON_REGISTRY_FAILURE,
        )
    }

    pub(super) fn require_recovery(
        &mut self,
        spec: &TransactionSpec,
        cause: RecoveryCause,
        reason: &str,
    ) -> Result<SwitchOutcome, RuntimeError> {
        if let Err(containment) = self
            .backend
            .acquire_gate(spec.managed_package())
            .and_then(|()| self.backend.verify_gate_held(spec.managed_package()))
            .and_then(|()| self.backend.quiesce_processes(spec.managed_package()))
        {
            return Err(RuntimeError::ContainmentFailed { cause, containment });
        }
        self.persist_recovery_required(spec, cause, reason)
    }

    fn persist_recovery_required(
        &self,
        spec: &TransactionSpec,
        cause: RecoveryCause,
        reason: &str,
    ) -> Result<SwitchOutcome, RuntimeError> {
        if let Err(source) = self.append(
            spec,
            JournalEvent::RecoveryRequired {
                reason: reason.to_owned(),
            },
        ) {
            return Err(RuntimeError::RecoveryMarkerFailed { cause, source });
        }
        Ok(SwitchOutcome::RecoveryRequired { cause })
    }

    pub(super) fn retire_completed_lease(
        &mut self,
        spec: &TransactionSpec,
    ) -> Result<Option<SwitchOutcome>, RuntimeError> {
        let Err(error) = self.backend.retire_gate_lease(spec.managed_package()) else {
            return Ok(None);
        };
        let cause = RecoveryCause::Platform(error);
        let already_contained = self
            .backend
            .verify_gate_held(spec.managed_package())
            .and_then(|()| self.backend.quiesce_processes(spec.managed_package()));
        let containment = already_contained.or_else(|_| {
            self.backend
                .acquire_gate(spec.managed_package())
                .and_then(|()| self.backend.verify_gate_held(spec.managed_package()))
                .and_then(|()| self.backend.quiesce_processes(spec.managed_package()))
        });
        if let Err(containment) = containment {
            return Err(RuntimeError::ContainmentFailed { cause, containment });
        }
        Ok(Some(SwitchOutcome::RecoveryRequired { cause }))
    }
}
