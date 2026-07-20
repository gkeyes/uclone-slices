use crate::lifecycle::GuardDecision;
use crate::runtime::{RuntimeError, SwitchOutcome};
use crate::service::{ServiceError, SwitchExecution};

pub(super) const fn runtime(error: &RuntimeError) -> ServiceError {
    match error {
        RuntimeError::GuardRejected(GuardDecision::Quarantine) => ServiceError::Quarantined,
        RuntimeError::GuardRejected(
            GuardDecision::RecoveryRequired(_)
            | GuardDecision::RequireSafeUpdateWindow
            | GuardDecision::AllowUpdateVerification
            | GuardDecision::AllowBase
            | GuardDecision::AllowSlot,
        )
        | RuntimeError::ContainmentFailed { .. }
        | RuntimeError::RecoveryMarkerFailed { .. }
        | RuntimeError::InjectedCrash(_) => ServiceError::RecoveryRequired,
        RuntimeError::Platform(_) | RuntimeError::Journal(_) | RuntimeError::Registry(_) => {
            ServiceError::Internal
        }
    }
}

pub(super) fn switch(outcome: SwitchOutcome) -> SwitchExecution {
    match outcome {
        SwitchOutcome::Committed { revision } => SwitchExecution::Committed(
            crate::domain::SlotView::new(revision.active_slot().clone(), revision.active_inodes()),
        ),
        SwitchOutcome::RolledBack { .. } => SwitchExecution::RolledBack,
        SwitchOutcome::RecoveryRequired { .. } => SwitchExecution::RecoveryRequired,
    }
}
