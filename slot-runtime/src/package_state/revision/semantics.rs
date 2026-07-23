use crate::lifecycle::LifecycleState;
use crate::package_state::{PackageStateError, PackageStateReason};

use super::PackageStateRevision;

pub(super) const fn is_update_state(state: LifecycleState) -> bool {
    matches!(
        state,
        LifecycleState::UpdatePreparing
            | LifecycleState::UpdateWindowOpen
            | LifecycleState::UpdateVerifying
    )
}

pub(super) fn verify_lifecycle_transition(
    previous: &PackageStateRevision,
    revision: &PackageStateRevision,
) -> Result<(), PackageStateError> {
    if !previous
        .lifecycle_state
        .can_transition_to(revision.lifecycle_state)
    {
        return Err(PackageStateError::Corrupt(format!(
            "illegal lifecycle transition from {:?} to {:?}",
            previous.lifecycle_state, revision.lifecycle_state
        )));
    }
    Ok(())
}

pub(super) fn reason_matches_v1(
    state: LifecycleState,
    reason: PackageStateReason,
    first: bool,
) -> bool {
    if first {
        return state == LifecycleState::Normal && reason == PackageStateReason::Enrolled;
    }
    reason_matches_non_normal(state, reason)
        || (state == LifecycleState::Normal && reason == PackageStateReason::ManualRepair)
}

pub(super) fn reason_matches_v2(
    previous: LifecycleState,
    state: LifecycleState,
    reason: PackageStateReason,
) -> bool {
    if state == LifecycleState::Normal {
        return matches!(
            (previous, reason),
            (
                LifecycleState::UpdateVerifying,
                PackageStateReason::ManagedUpdate
            ) | (
                LifecycleState::RepairWaiting,
                PackageStateReason::ManualRepair
            )
        );
    }
    reason_matches_non_normal(state, reason)
}

fn reason_matches_non_normal(state: LifecycleState, reason: PackageStateReason) -> bool {
    match reason {
        PackageStateReason::Enrolled => false,
        PackageStateReason::ManagedUpdate => matches!(
            state,
            LifecycleState::UpdatePreparing
                | LifecycleState::UpdateWindowOpen
                | LifecycleState::UpdateVerifying
        ),
        PackageStateReason::IdentityChanged => state == LifecycleState::Quarantined,
        PackageStateReason::ViewUncertain => state == LifecycleState::RecoveryRequired,
        PackageStateReason::LifecycleDrift => matches!(
            state,
            LifecycleState::LifecycleDrifted | LifecycleState::RecoveryRequired
        ),
        PackageStateReason::ManualRepair => state == LifecycleState::RepairWaiting,
    }
}
