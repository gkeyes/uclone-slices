use crate::journal::{JournalEvent, TransactionSpec};
use crate::lifecycle::{GuardDecision, PackageLifecycleGuard};
use crate::registry::PackageRevision;

use super::{REASON_JOURNAL_FAILURE, REASON_PLATFORM_FAILURE, SwitchCoordinator};
use crate::runtime::{
    FaultPoint, RecoveryCause, RuntimeBackend, RuntimeError, SwitchOutcome, SwitchRequest,
};

impl<B: RuntimeBackend> SwitchCoordinator<B> {
    pub(super) fn execute_gated(
        &mut self,
        request: &SwitchRequest,
        spec: &TransactionSpec,
    ) -> Result<SwitchOutcome, RuntimeError> {
        if let Err(error) = self.backend.quiesce_processes(request.managed_package()) {
            return self.rollback_platform(spec, error);
        }
        if let Err(error) = self.append(spec, JournalEvent::ProcessesQuiesced) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::ProcessesQuiesced)?;
        let observation = match self.backend.observe_package(request.managed_package()) {
            Ok(value) => value,
            Err(error) => {
                return self.require_recovery(
                    spec,
                    RecoveryCause::Platform(error),
                    REASON_PLATFORM_FAILURE,
                );
            }
        };
        match PackageLifecycleGuard::assess(request.managed_package(), &observation) {
            GuardDecision::AllowBase | GuardDecision::AllowSlot => {}
            rejected => return self.reject_lifecycle(spec, rejected),
        }
        if let Err(error) = self.backend.verify_gate_held(request.managed_package()) {
            return self.require_recovery(
                spec,
                RecoveryCause::Platform(error),
                REASON_PLATFORM_FAILURE,
            );
        }
        if let Err(error) = self.append(spec, JournalEvent::Applying) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::Applying)?;
        if let Err(error) = self
            .backend
            .apply_slot_view(request.managed_package(), request.target_view())
        {
            return self.rollback_platform(spec, error);
        }
        self.faults.check(FaultPoint::TargetApplied)?;
        if let Err(error) = self
            .backend
            .verify_slot_view(request.managed_package(), request.target_view())
        {
            return self.rollback_platform(spec, error);
        }
        if let Err(error) = self.append(spec, JournalEvent::ViewVerified) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::ViewVerified)?;
        self.commit(request, spec)
    }

    fn reject_lifecycle(
        &mut self,
        spec: &TransactionSpec,
        decision: GuardDecision,
    ) -> Result<SwitchOutcome, RuntimeError> {
        let reason = if decision == GuardDecision::Quarantine {
            "identity_changed"
        } else {
            "lifecycle_drift"
        };
        if let Err(error) = self.append(
            spec,
            JournalEvent::RecoveryRequired {
                reason: reason.to_owned(),
            },
        ) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        Err(RuntimeError::GuardRejected(decision))
    }

    fn commit(
        &mut self,
        request: &SwitchRequest,
        spec: &TransactionSpec,
    ) -> Result<SwitchOutcome, RuntimeError> {
        let nonce = request.metadata().commit_nonce().clone();
        if let Err(error) = self.append(
            spec,
            JournalEvent::Committing {
                nonce: nonce.clone(),
            },
        ) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::Committing)?;
        let draft = match PackageRevision::committed(spec, spec.base_inodes(), nonce.clone()) {
            Ok(value) => value,
            Err(error) => return self.registry_recovery(spec, error),
        };
        let revision = match self.stores.registry().append(&draft) {
            Ok(value) => value,
            Err(error) => return self.registry_recovery(spec, error),
        };
        self.faults.check(FaultPoint::RegistryPublished)?;
        if let Err(error) = self.append(spec, JournalEvent::RegistryCommitted { nonce }) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::RegistryCommitted)?;
        if let Err(error) = self
            .backend
            .restore_gate(request.managed_package(), spec.gate_snapshot())
        {
            return self.require_recovery(
                spec,
                RecoveryCause::Platform(error),
                REASON_PLATFORM_FAILURE,
            );
        }
        self.faults.check(FaultPoint::GateRestored)?;
        if let Err(error) = self.append(spec, JournalEvent::GateReleased) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::GateReleased)?;
        if let Err(error) = self.append(spec, JournalEvent::Completed) {
            return self.require_recovery(
                spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::Completed)?;
        if let Some(outcome) = self.retire_completed_lease(spec)? {
            return Ok(outcome);
        }
        self.faults.check(FaultPoint::GateLeaseRetired)?;
        Ok(SwitchOutcome::Committed {
            revision: Box::new(revision),
        })
    }
}
