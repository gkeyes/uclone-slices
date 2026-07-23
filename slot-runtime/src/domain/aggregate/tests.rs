use super::*;

fn ready_facts(scope: EvidenceScope) -> PackageFacts {
    PackageFacts::new(
        scope,
        DurablePackageFacts::Enrolled {
            lifecycle: LifecycleState::Normal,
            active_slot: SlotId::base(),
        },
        LivePackageFacts::Healthy,
        GateFacts::Observed,
    )
}

#[test]
fn unlocked_ready_package_admits_slot_mutation_and_launch() {
    let aggregate = PackageAggregate::resolve(ready_facts(EvidenceScope::Unlocked));

    assert_eq!(aggregate.state(), AggregateState::Ready);
    assert_eq!(aggregate.safety(), SafetyDisposition::PreserveObservedGate);
    assert!(
        aggregate
            .allowed_actions()
            .contains(&AllowedAction::MutateSlots)
    );
    assert!(aggregate.allowed_actions().contains(&AllowedAction::Launch));
}

#[test]
fn recovery_only_ready_package_never_admits_slot_mutation_or_launch() {
    let aggregate = PackageAggregate::resolve(ready_facts(EvidenceScope::RecoveryOnly));

    assert_eq!(aggregate.state(), AggregateState::Ready);
    assert_eq!(aggregate.safety(), SafetyDisposition::HoldGate);
    assert!(
        !aggregate
            .allowed_actions()
            .contains(&AllowedAction::MutateSlots)
    );
    assert!(!aggregate.allowed_actions().contains(&AllowedAction::Launch));
}

#[test]
fn incomplete_or_transitional_evidence_is_fail_closed() {
    let incomplete = PackageAggregate::resolve(PackageFacts::new(
        EvidenceScope::Unlocked,
        DurablePackageFacts::Incomplete(InvariantViolation::RegistryJournalMismatch),
        LivePackageFacts::NotObserved,
        GateFacts::NotObserved,
    ));
    let transitional = PackageAggregate::resolve(PackageFacts::new(
        EvidenceScope::Unlocked,
        DurablePackageFacts::Enrolled {
            lifecycle: LifecycleState::UpdateVerifying,
            active_slot: SlotId::base(),
        },
        LivePackageFacts::Healthy,
        GateFacts::Observed,
    ));

    assert_eq!(incomplete.state(), AggregateState::RecoveryRequired);
    assert_eq!(incomplete.safety(), SafetyDisposition::HoldGate);
    assert_eq!(
        transitional.violation(),
        Some(InvariantViolation::PersistedLifecycleState)
    );
}

#[test]
fn identity_violation_is_quarantined() {
    let aggregate = PackageAggregate::resolve(PackageFacts::new(
        EvidenceScope::Unlocked,
        DurablePackageFacts::Enrolled {
            lifecycle: LifecycleState::Normal,
            active_slot: SlotId::base(),
        },
        LivePackageFacts::Quarantined(InvariantViolation::IdentityChanged),
        GateFacts::NotObserved,
    ));

    assert_eq!(aggregate.state(), AggregateState::Quarantined);
    assert_eq!(aggregate.safety(), SafetyDisposition::Quarantine);
}
