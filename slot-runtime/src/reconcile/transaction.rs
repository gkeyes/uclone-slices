use crate::domain::{ManagedPackage, PackageKey, SlotView, TransactionId};
use crate::journal::{JournalEvent, JournalStep, Transaction, TransactionSpec};
use crate::recovery::{RecoveryDecision, decide_recovery};
use crate::registry::PackageRevision;

use super::backend::RecoveryBackend;
use super::coordinator::Reconciler;
use super::error::ReconcileError;
use super::model::{HeldPackage, ReconcileOutcome, ReconcileReason};

impl<B: RecoveryBackend> Reconciler<B> {
    pub(super) fn reconcile_transaction(
        &mut self,
        held: &HeldPackage,
        transaction_id: &TransactionId,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        let Ok(transaction) = self.journal.load(transaction_id) else {
            return self.require_recovery(held, ReconcileReason::JournalMetadata);
        };
        if !transaction_matches_enrollment(&transaction, &held.managed) {
            return self.require_recovery(held, ReconcileReason::JournalMetadata);
        }
        let Ok(latest) = self.registry.latest(transaction.spec().package_name()) else {
            return self.require_recovery(held, ReconcileReason::RegistryMetadata);
        };
        match decide_recovery(&transaction, latest.as_ref()) {
            RecoveryDecision::RollbackToPrevious => {
                self.rollback_transaction(held, &transaction, latest.as_ref())
            }
            RecoveryDecision::RollForwardToTarget => {
                self.roll_forward_transaction(held, &transaction)
            }
            RecoveryDecision::NoAction => self.complete_proven_transaction(held, &transaction),
            RecoveryDecision::RecoveryRequired => {
                self.require_recovery(held, ReconcileReason::CommitPointUncertain)
            }
        }
    }

    fn rollback_transaction(
        &mut self,
        held: &HeldPackage,
        transaction: &Transaction,
        latest: Option<&PackageRevision>,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        if !self.begin_rollback(transaction, latest) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        let previous = SlotView::new(
            transaction.spec().previous_slot().clone(),
            transaction.spec().previous_inodes(),
        );
        let Some(managed) = package_for_view(transaction.spec(), &previous) else {
            return self.require_recovery(held, ReconcileReason::JournalMetadata);
        };
        if !self.restore_view(&managed, &previous) {
            return self.require_recovery(held, ReconcileReason::ViewRestoreFailed);
        }
        if !self.finish_rolled_back(transaction.spec().transaction_id()) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        if let Err(reason) = self.prove_release_ready(&managed, &previous) {
            return self.require_recovery(held, reason);
        }
        if !self.restore_gate(held) {
            return self.require_recovery(held, ReconcileReason::GateRestoreFailed);
        }
        if !self.finish_gate_release(transaction.spec().transaction_id()) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        if let Some(recovery) = self.retire_gate_or_recover(held)? {
            return Ok(recovery);
        }
        Ok(ReconcileOutcome::RolledBack)
    }

    fn roll_forward_transaction(
        &mut self,
        held: &HeldPackage,
        transaction: &Transaction,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        let target = SlotView::new(
            transaction.spec().target_slot().clone(),
            transaction.spec().target_inodes(),
        );
        let Some(managed) = package_for_view(transaction.spec(), &target) else {
            return self.require_recovery(held, ReconcileReason::JournalMetadata);
        };
        if !self.restore_view(&managed, &target) {
            return self.require_recovery(held, ReconcileReason::ViewRestoreFailed);
        }
        if !self.acknowledge_registry_commit(transaction) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        if let Err(reason) = self.prove_release_ready(&managed, &target) {
            return self.require_recovery(held, reason);
        }
        if !self.restore_gate(held) {
            return self.require_recovery(held, ReconcileReason::GateRestoreFailed);
        }
        if !self.finish_gate_release(transaction.spec().transaction_id()) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        if let Some(recovery) = self.retire_gate_or_recover(held)? {
            return Ok(recovery);
        }
        Ok(ReconcileOutcome::RolledForward)
    }

    fn begin_rollback(&self, transaction: &Transaction, latest: Option<&PackageRevision>) -> bool {
        let Some(last) = transaction.steps().last().map(JournalStep::event) else {
            return false;
        };
        let transaction_id = transaction.spec().transaction_id();
        match last {
            JournalEvent::Prepared { .. } => self
                .journal
                .append(transaction_id, JournalEvent::GateHeld)
                .and_then(|_| {
                    self.journal
                        .append(transaction_id, JournalEvent::RollingBack)
                })
                .is_ok(),
            JournalEvent::GateHeld
            | JournalEvent::ProcessesQuiesced
            | JournalEvent::Applying
            | JournalEvent::ViewVerified => self
                .journal
                .append(transaction_id, JournalEvent::RollingBack)
                .is_ok(),
            JournalEvent::Committing { .. } => self
                .journal
                .append_proven_rollback(transaction_id, latest)
                .is_ok(),
            JournalEvent::RollingBack | JournalEvent::RolledBack => true,
            JournalEvent::RecoveryRequired { .. }
                if transaction.recoverable_platform_precommit() =>
            {
                self.journal
                    .append(transaction_id, JournalEvent::RollingBack)
                    .is_ok()
            }
            JournalEvent::RegistryCommitted { .. }
            | JournalEvent::GateReleased
            | JournalEvent::Completed
            | JournalEvent::RecoveryRequired { .. } => false,
        }
    }
}

fn transaction_matches_enrollment(transaction: &Transaction, enrolled: &ManagedPackage) -> bool {
    let spec = transaction.spec();
    spec.package_name() == enrolled.package_name()
        && spec.user_id() == enrolled.user_id()
        && spec.identity() == enrolled.identity()
        && spec.lifecycle_state() == enrolled.lifecycle_state()
        && spec.base_inodes() == enrolled.base_inodes()
}

pub(super) fn package_for_view(spec: &TransactionSpec, view: &SlotView) -> Option<ManagedPackage> {
    ManagedPackage::new(
        PackageKey::new(spec.package_name().clone(), spec.user_id()),
        spec.identity().clone(),
        spec.base_inodes(),
        view.clone(),
        spec.lifecycle_state(),
    )
    .ok()
}
