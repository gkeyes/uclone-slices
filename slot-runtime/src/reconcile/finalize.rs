use crate::domain::{SlotView, TransactionId};
use crate::journal::{JournalEvent, JournalStep, Transaction, TransactionView};

use super::backend::RecoveryBackend;
use super::coordinator::Reconciler;
use super::error::ReconcileError;
use super::model::{HeldPackage, ReconcileOutcome, ReconcileReason};
use super::transaction::package_for_view;

impl<B: RecoveryBackend> Reconciler<B> {
    pub(super) fn complete_proven_transaction(
        &mut self,
        held: &HeldPackage,
        transaction: &Transaction,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        let (view, outcome) = match transaction.view() {
            TransactionView::CompletedTarget => (
                SlotView::new(
                    transaction.spec().target_slot().clone(),
                    transaction.spec().target_inodes(),
                ),
                ReconcileOutcome::RolledForward,
            ),
            TransactionView::CompletedPrevious => (
                SlotView::new(
                    transaction.spec().previous_slot().clone(),
                    transaction.spec().previous_inodes(),
                ),
                ReconcileOutcome::RolledBack,
            ),
            TransactionView::PreCommit
            | TransactionView::CommitPending
            | TransactionView::PostCommit
            | TransactionView::RolledBack
            | TransactionView::RecoveryRequired => {
                return self.require_recovery(held, ReconcileReason::CommitPointUncertain);
            }
        };
        let Some(managed) = package_for_view(transaction.spec(), &view) else {
            return self.require_recovery(held, ReconcileReason::JournalMetadata);
        };
        if !self.restore_view(&managed, &view) {
            return self.require_recovery(held, ReconcileReason::ViewRestoreFailed);
        }
        if let Err(reason) = self.prove_release_ready(&managed, &view) {
            return self.require_recovery(held, reason);
        }
        if !self.restore_gate(held, &managed) {
            return self.require_recovery(held, ReconcileReason::GateRestoreFailed);
        }
        if !self.finish_gate_release(transaction.spec().transaction_id()) {
            return self.require_recovery(held, ReconcileReason::JournalUpdateFailed);
        }
        if let Some(recovery) = self.retire_gate_or_recover(held)? {
            return Ok(recovery);
        }
        Ok(outcome)
    }

    pub(super) fn finish_rolled_back(&self, transaction_id: &TransactionId) -> bool {
        let Ok(transaction) = self.journal.load(transaction_id) else {
            return false;
        };
        match transaction.steps().last().map(JournalStep::event) {
            Some(JournalEvent::RollingBack) => self
                .journal
                .append(transaction_id, JournalEvent::RolledBack)
                .is_ok(),
            Some(JournalEvent::RolledBack) => true,
            Some(
                JournalEvent::Prepared { .. }
                | JournalEvent::GateHeld
                | JournalEvent::ProcessesQuiesced
                | JournalEvent::Applying
                | JournalEvent::ViewVerified
                | JournalEvent::Committing { .. }
                | JournalEvent::RegistryCommitted { .. }
                | JournalEvent::GateReleased
                | JournalEvent::Completed
                | JournalEvent::RecoveryRequired { .. },
            )
            | None => false,
        }
    }

    pub(super) fn acknowledge_registry_commit(&self, transaction: &Transaction) -> bool {
        let transaction_id = transaction.spec().transaction_id();
        match transaction.steps().last().map(JournalStep::event) {
            Some(JournalEvent::Committing { nonce }) => self
                .journal
                .append(
                    transaction_id,
                    JournalEvent::RegistryCommitted {
                        nonce: nonce.clone(),
                    },
                )
                .is_ok(),
            Some(JournalEvent::RegistryCommitted { .. } | JournalEvent::GateReleased) => true,
            Some(
                JournalEvent::Prepared { .. }
                | JournalEvent::GateHeld
                | JournalEvent::ProcessesQuiesced
                | JournalEvent::Applying
                | JournalEvent::ViewVerified
                | JournalEvent::Completed
                | JournalEvent::RollingBack
                | JournalEvent::RolledBack
                | JournalEvent::RecoveryRequired { .. },
            )
            | None => false,
        }
    }

    pub(super) fn finish_gate_release(&self, transaction_id: &TransactionId) -> bool {
        let Ok(transaction) = self.journal.load(transaction_id) else {
            return false;
        };
        match transaction.steps().last().map(JournalStep::event) {
            Some(JournalEvent::RegistryCommitted { .. } | JournalEvent::RolledBack) => self
                .journal
                .append(transaction_id, JournalEvent::GateReleased)
                .and_then(|_| self.journal.append(transaction_id, JournalEvent::Completed))
                .is_ok(),
            Some(JournalEvent::GateReleased) => self
                .journal
                .append(transaction_id, JournalEvent::Completed)
                .is_ok(),
            Some(JournalEvent::Completed) => true,
            _ => false,
        }
    }
}
