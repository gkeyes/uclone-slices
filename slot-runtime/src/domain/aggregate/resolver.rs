use crate::lifecycle::LifecycleState;

use super::super::SlotId;
use super::{
    AggregateKind, AllowedAction, DurablePackageFacts, EvidenceScope, GateFacts,
    InvariantViolation, LivePackageFacts, PackageAggregate, PackageFacts, ReadyPackageEvidence,
    SafetyDisposition,
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
        } => resolve_enrolled(
            facts.scope,
            lifecycle,
            active_slot,
            facts.live,
            facts.gate,
            facts.ready_evidence,
        ),
    }
}

fn resolve_enrolled(
    scope: EvidenceScope,
    lifecycle: LifecycleState,
    active_slot: SlotId,
    live: LivePackageFacts,
    gate: GateFacts,
    ready_evidence: Option<ReadyPackageEvidence>,
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
        LivePackageFacts::Healthy if gate == GateFacts::Observed => {
            let Some(evidence) = ready_evidence else {
                return recovery(scope, InvariantViolation::ReadyEvidenceUnavailable);
            };
            if evidence.managed().active_slot() != &active_slot {
                return recovery(scope, InvariantViolation::ActiveViewUnproven);
            }
            ready(scope, evidence)
        }
        LivePackageFacts::Healthy => {
            recovery(scope, InvariantViolation::GateObservationUnavailable)
        }
    }
}

fn absent(scope: EvidenceScope) -> PackageAggregate {
    PackageAggregate {
        kind: AggregateKind::Absent,
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

fn ready(scope: EvidenceScope, evidence: ReadyPackageEvidence) -> PackageAggregate {
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
        kind: AggregateKind::Ready(evidence),
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
        kind: AggregateKind::RecoveryRequired,
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
        kind: AggregateKind::Quarantined,
        safety: SafetyDisposition::Quarantine,
        allowed_actions: vec![AllowedAction::Inspect, AllowedAction::RescueToBase],
        violation: Some(violation),
    }
}
