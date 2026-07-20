use super::{JournalEvent, JournalStep, TransactionSpec};
use crate::domain::CommitNonce;

#[doc = "Verified transaction reconstructed from immutable steps."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    spec: TransactionSpec,
    steps: Vec<JournalStep>,
}

impl Transaction {
    pub(super) const fn new(spec: TransactionSpec, steps: Vec<JournalStep>) -> Self {
        Self { spec, steps }
    }

    #[doc = "Returns the immutable transaction specification."]
    pub const fn spec(&self) -> &TransactionSpec {
        &self.spec
    }

    #[doc = "Returns every verified step in generation order."]
    pub fn steps(&self) -> &[JournalStep] {
        &self.steps
    }

    #[doc = "Classifies which side of the Registry commit point was reached."]
    pub fn view(&self) -> TransactionView {
        let Some(last) = self.steps.last() else {
            return TransactionView::RecoveryRequired;
        };
        match last.event() {
            JournalEvent::Prepared { .. }
            | JournalEvent::GateHeld
            | JournalEvent::ProcessesQuiesced
            | JournalEvent::Applying
            | JournalEvent::ViewVerified
            | JournalEvent::RollingBack => TransactionView::PreCommit,
            JournalEvent::Committing { .. } => TransactionView::CommitPending,
            JournalEvent::RegistryCommitted { .. } => TransactionView::PostCommit,
            JournalEvent::GateReleased | JournalEvent::Completed => self.completed_view(),
            JournalEvent::RolledBack => TransactionView::RolledBack,
            JournalEvent::RecoveryRequired { .. } => TransactionView::RecoveryRequired,
        }
    }

    #[doc = "Returns the nonce declared before Registry commit, if reached."]
    pub fn committing_nonce(&self) -> Option<&CommitNonce> {
        self.steps.iter().rev().find_map(|step| match step.event() {
            JournalEvent::Committing { nonce } => Some(nonce),
            _ => None,
        })
    }

    #[doc = "Returns the nonce acknowledged after Registry commit, if recorded."]
    pub fn registry_committed_nonce(&self) -> Option<&CommitNonce> {
        self.steps.iter().rev().find_map(|step| match step.event() {
            JournalEvent::RegistryCommitted { nonce } => Some(nonce),
            _ => None,
        })
    }

    #[doc = "Returns true only for a platform failure recorded before the Registry commit point."]
    pub fn recoverable_platform_precommit(&self) -> bool {
        let [.., previous, latest] = self.steps.as_slice() else {
            return false;
        };
        matches!(
            latest.event(),
            JournalEvent::RecoveryRequired { reason } if reason == "platform_failure"
        ) && matches!(
            previous.event(),
            JournalEvent::Prepared { .. }
                | JournalEvent::GateHeld
                | JournalEvent::ProcessesQuiesced
                | JournalEvent::Applying
                | JournalEvent::ViewVerified
        ) && self.committing_nonce().is_none()
            && self.registry_committed_nonce().is_none()
    }

    fn completed_view(&self) -> TransactionView {
        if self
            .steps
            .iter()
            .any(|step| matches!(step.event(), JournalEvent::RegistryCommitted { .. }))
        {
            TransactionView::CompletedTarget
        } else {
            TransactionView::CompletedPrevious
        }
    }
}

#[doc = "Coarse recovery position relative to the Registry commit point."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionView {
    #[doc = "The Registry commit point has not been declared."]
    PreCommit,
    #[doc = "A nonce was declared but Registry durability is unknown."]
    CommitPending,
    #[doc = "The journal records the durable Registry commit."]
    PostCommit,
    #[doc = "The target view and original package gate were durably completed."]
    CompletedTarget,
    #[doc = "The previous view was restored and its original package gate was released."]
    CompletedPrevious,
    #[doc = "The previous slot was restored while the gate remains held."]
    RolledBack,
    #[doc = "The journal itself records an unprovable state."]
    RecoveryRequired,
}
