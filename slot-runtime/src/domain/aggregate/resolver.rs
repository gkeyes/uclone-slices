use crate::lifecycle::LifecycleState;

use super::{
    AggregateState, AllowedAction, DurablePackageFacts, EvidenceScope, GateFacts,
    InvariantViolation, LivePackageFacts, PackageAggregate, PackageFacts, SafetyDisposition,
    SlotId,
};

pub(super) fn resolve(facts: PackageFacts) -> PackageAggregate {
    match facts.durable {
        DurablePackageFacts::Absent => absent(facts.scope),
        DurablePackageFacts::EnrollmentAttempt => {
            recovery(facts.scope, InvariantViolation::EnrollmentAttemptPending)
        }
        DurablePackageFacts::Orphaned => {
            recovery(facts.scope, InvariantViolation::OrphanedManagementEvidence)
        }
        DurablePackageFacts::Incomplete(violation) => recovery(facts.scope, violation),
        DurablePackageFacts::Enrolled {
            lifecycle,
            active_slot,
        } => resolve_enrolled(facts.scope, lifecycle, active_slot, facts.live, facts.gate),
    }
}

fn resolve_enrolled(
    scope: EvidenceScope,
    lifecycle: LifecycleState,
    active_slot: SlotId,
    live: LivePackageFacts,
    gate: GateFacts,
) -> PackageAggregate {
    if lifecycle == LifecycleState::Quarantined {
        return quarantined(InvariantViolation::IdentityChanged);
    }
    if lifecycle != LifecycleState::Normal {
        return recovery(scope, InvariantViolation::PersistedLifecycleState);
    }
    match live {
        LivePackageFacts::Quarantined(violation) => quarantined(violation),
        LivePackageFacts::RecoveryRequired(violation) => recovery(scope, violation),
        LivePackageFacts::NotObserved => {
            recovery(scope, InvariantViolation::LiveObservationUnavailable)
        }
        LivePackageFacts::Healthy if gate == GateFacts::Observed => ready(scope, active_slot),
        LivePackageFacts::Healthy => {
            recovery(scope, InvariantViolation::GateObservationUnavailable)
        }
    }
}

fn absent(scope: EvidenceScope) -> PackageAggregate {
    PackageAggregate {
        state: AggregateState::Absent,
        active_slot: None,
        safety: SafetyDisposition::NoContainmentRequired,
        allowed_actions: match scope {
            EvidenceScope::Unlocked => vec![AllowedAction::Inspect, AllowedAction::Enroll],
            EvidenceScope::EarlyBootLocked | EvidenceScope::RecoveryOnly => {
                vec![AllowedAction::Inspect]
            }
        },
        violation: None,
    }
}

fn ready(scope: EvidenceScope, active_slot: SlotId) -> PackageAggregate {
    let allowed_actions = match scope {
        EvidenceScope::Unlocked => vec![
            AllowedAction::Inspect,
            AllowedAction::ReadStatus,
            AllowedAction::MutateSlots,
            AllowedAction::Reconcile,
            AllowedAction::Launch,
        ],
        EvidenceScope::EarlyBootLocked | EvidenceScope::RecoveryOnly => vec![
            AllowedAction::Inspect,
            AllowedAction::ReadStatus,
            AllowedAction::Reconcile,
            AllowedAction::RescueToBase,
        ],
    };
    PackageAggregate {
        state: AggregateState::Ready,
        active_slot: Some(active_slot),
        safety: match scope {
            EvidenceScope::Unlocked => SafetyDisposition::PreserveObservedGate,
            EvidenceScope::EarlyBootLocked | EvidenceScope::RecoveryOnly => {
                SafetyDisposition::HoldGate
            }
        },
        allowed_actions,
        violation: None,
    }
}

fn recovery(scope: EvidenceScope, violation: InvariantViolation) -> PackageAggregate {
    PackageAggregate {
        state: AggregateState::RecoveryRequired,
        active_slot: None,
        safety: SafetyDisposition::HoldGate,
        allowed_actions: match scope {
            EvidenceScope::EarlyBootLocked => {
                vec![AllowedAction::Inspect, AllowedAction::Reconcile]
            }
            EvidenceScope::Unlocked | EvidenceScope::RecoveryOnly => vec![
                AllowedAction::Inspect,
                AllowedAction::Reconcile,
                AllowedAction::RescueToBase,
            ],
        },
        violation: Some(violation),
    }
}

fn quarantined(violation: InvariantViolation) -> PackageAggregate {
    PackageAggregate {
        state: AggregateState::Quarantined,
        active_slot: None,
        safety: SafetyDisposition::Quarantine,
        allowed_actions: vec![AllowedAction::Inspect, AllowedAction::RescueToBase],
        violation: Some(violation),
    }
}
