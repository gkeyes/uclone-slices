#![doc = "Deterministic reconciliation across journal and Registry crash points."]

use crate::journal::{Transaction, TransactionView};
use crate::registry::PackageRevision;

#[doc = "One fail-closed reconciliation outcome."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDecision {
    #[doc = "Restore and verify the transaction's previous slot."]
    RollbackToPrevious,
    #[doc = "Finish and verify the transaction's target slot."]
    RollForwardToTarget,
    #[doc = "No filesystem or gate action remains."]
    NoAction,
    #[doc = "Neither side can be proved; retain the execution gate."]
    RecoveryRequired,
}

#[doc = "Chooses recovery using the journal declaration and Registry commit point."]
pub fn decide_recovery(
    transaction: &Transaction,
    latest: Option<&PackageRevision>,
) -> RecoveryDecision {
    match transaction.view() {
        TransactionView::PreCommit | TransactionView::RolledBack => {
            if registry_matches_target(transaction, latest) {
                RecoveryDecision::RecoveryRequired
            } else if registry_matches_previous(transaction, latest) {
                RecoveryDecision::RollbackToPrevious
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
        TransactionView::CommitPending => {
            if registry_matches_target(transaction, latest) {
                RecoveryDecision::RollForwardToTarget
            } else if registry_matches_previous(transaction, latest) {
                RecoveryDecision::RollbackToPrevious
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
        TransactionView::PostCommit => {
            if registry_matches_target(transaction, latest) {
                RecoveryDecision::RollForwardToTarget
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
        TransactionView::CompletedTarget => {
            if registry_matches_target(transaction, latest) {
                RecoveryDecision::NoAction
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
        TransactionView::CompletedPrevious => {
            if registry_matches_previous(transaction, latest) {
                RecoveryDecision::NoAction
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
        TransactionView::RecoveryRequired => {
            if transaction.recoverable_platform_precommit()
                && registry_matches_previous(transaction, latest)
            {
                RecoveryDecision::RollbackToPrevious
            } else {
                RecoveryDecision::RecoveryRequired
            }
        }
    }
}

fn registry_matches_target(transaction: &Transaction, latest: Option<&PackageRevision>) -> bool {
    let Some(revision) = latest else {
        return false;
    };
    let Some(nonce) = transaction.committing_nonce() else {
        return false;
    };
    revision.package_name() == transaction.spec().package_name()
        && revision.user_id() == transaction.spec().user_id()
        && revision.identity() == transaction.spec().identity()
        && revision.lifecycle_state() == transaction.spec().lifecycle_state()
        && revision.base_inodes() == transaction.spec().base_inodes()
        && revision.transaction_id() == transaction.spec().transaction_id()
        && revision.commit_nonce() == nonce
        && revision.active_slot() == transaction.spec().target_slot()
        && revision.active_inodes() == transaction.spec().target_inodes()
}

fn registry_matches_previous(transaction: &Transaction, latest: Option<&PackageRevision>) -> bool {
    latest.map_or_else(
        || transaction.spec().previous_slot().is_base(),
        |revision| {
            revision.package_name() == transaction.spec().package_name()
                && revision.user_id() == transaction.spec().user_id()
                && revision.identity() == transaction.spec().identity()
                && revision.lifecycle_state() == transaction.spec().lifecycle_state()
                && revision.base_inodes() == transaction.spec().base_inodes()
                && revision.active_slot() == transaction.spec().previous_slot()
                && revision.active_inodes() == transaction.spec().previous_inodes()
        },
    )
}
