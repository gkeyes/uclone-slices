#![allow(
    clippy::unwrap_used,
    reason = "validated aggregate fixture construction must abort the individual test on failure"
)]

use super::*;
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, UserId,
};
use crate::lifecycle::LifecycleState;

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn ready_evidence() -> ReadyPackageEvidence {
    let base_inodes = DataInodes::new(100, 200).unwrap();
    ReadyPackageEvidence::new(
        ManagedPackage::new(
            PackageKey::new(
                PackageName::parse("com.uclone.slotprobe").unwrap(),
                UserId::PRIMARY,
            ),
            AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
            base_inodes,
            SlotView::new(SlotId::base(), base_inodes),
            LifecycleState::Normal,
        )
        .unwrap(),
        Vec::new(),
        GateSnapshot::new(PackageEnabledState::Enabled, false),
    )
}

fn ready_facts(scope: EvidenceScope) -> PackageFacts {
    PackageFacts::with_ready_evidence(
        scope,
        DurablePackageFacts::Enrolled {
            lifecycle: LifecycleState::Normal,
            active_slot: SlotId::base(),
        },
        LivePackageFacts::Healthy,
        GateFacts::Observed,
        ready_evidence(),
    )
}

#[test]
fn unlocked_ready_package_admits_slot_mutation_and_launch() {
    let aggregate = PackageAggregate::resolve(ready_facts(EvidenceScope::Unlocked));

    assert_eq!(aggregate.state(), AggregateState::Ready);
    assert_eq!(aggregate.safety(), SafetyDisposition::PreserveObservedGate);
    assert_eq!(
        aggregate.ready_evidence().map(ReadyPackageEvidence::gate),
        Some(GateSnapshot::new(PackageEnabledState::Enabled, false))
    );
    assert!(
        aggregate
            .allowed_actions()
            .contains(&AllowedAction::MutateSlots)
    );
    assert!(aggregate.allowed_actions().contains(&AllowedAction::Launch));
}

#[test]
fn restricted_scopes_keep_ready_packages_gated_and_non_mutating() {
    for scope in [EvidenceScope::EarlyBootLocked, EvidenceScope::RecoveryOnly] {
        let aggregate = PackageAggregate::resolve(ready_facts(scope));

        assert_eq!(aggregate.state(), AggregateState::Ready);
        assert_eq!(aggregate.safety(), SafetyDisposition::HoldGate);
        assert_eq!(
            aggregate.allowed_actions(),
            &[
                AllowedAction::Inspect,
                AllowedAction::ReadStatus,
                AllowedAction::Reconcile,
                AllowedAction::RescueToBase,
            ]
        );
    }
}

#[test]
fn restricted_scopes_make_recovery_permissions_explicit() {
    for (scope, expected_actions) in [
        (
            EvidenceScope::EarlyBootLocked,
            &[AllowedAction::Inspect, AllowedAction::Reconcile] as &[AllowedAction],
        ),
        (
            EvidenceScope::RecoveryOnly,
            &[
                AllowedAction::Inspect,
                AllowedAction::Reconcile,
                AllowedAction::RescueToBase,
            ] as &[AllowedAction],
        ),
    ] {
        let aggregate = PackageAggregate::resolve(PackageFacts::new(
            scope,
            DurablePackageFacts::Incomplete(InvariantViolation::RegistryJournalMismatch),
            LivePackageFacts::NotObserved,
            GateFacts::NotObserved,
        ));

        assert_eq!(aggregate.state(), AggregateState::RecoveryRequired);
        assert_eq!(aggregate.safety(), SafetyDisposition::HoldGate);
        assert_eq!(aggregate.allowed_actions(), expected_actions);
    }
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
    assert!(incomplete.ready_evidence().is_none());
    assert!(transitional.ready_evidence().is_none());
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
    assert!(aggregate.ready_evidence().is_none());
}
