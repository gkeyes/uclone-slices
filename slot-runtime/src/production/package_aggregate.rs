use crate::android::PackageProbe;
use crate::domain::{
    DurablePackageFacts, EvidenceScope, GateFacts, InvariantViolation, LivePackageFacts,
    ManagedPackage, PackageAggregate, PackageFacts, PackageKey,
};
use crate::lifecycle::{GuardDecision, LifecycleState, PackageLifecycleGuard, RecoveryReason};

use super::state::{
    active_slot_metadata_ready, active_view, catalog_views, has_unfinished,
    registry_matches_journal,
};
use super::stores::ProductionStores;

pub(super) fn resolve<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
    scope: EvidenceScope,
) -> PackageAggregate {
    PackageAggregate::resolve(load_facts(stores, probe, key, scope))
}

fn load_facts<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
    scope: EvidenceScope,
) -> PackageFacts {
    let Ok(attempt) = stores.attempts.load(key) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(enrollment) = stores.enrollment.load(key.package_name()) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(catalog) = stores.catalog.list(key) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(state) = stores.package_state.latest(key) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(registry) = stores.registry.latest(key.package_name()) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(journal) = stores.journal.list_for_package(key) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };
    let Ok(policy) = stores.compatibility_policy.load(key.package_name()) else {
        return incomplete(scope, InvariantViolation::StoreUnreadable);
    };

    if attempt.is_some() {
        return facts(scope, DurablePackageFacts::EnrollmentAttempt);
    }
    let Some(enrolled) = enrollment else {
        let clean = catalog.is_empty()
            && state.is_none()
            && registry.is_none()
            && policy.is_none()
            && super::state::journal_for(&journal, key).next().is_none();
        return facts(
            scope,
            if clean {
                DurablePackageFacts::Absent
            } else {
                DurablePackageFacts::Orphaned
            },
        );
    };
    let Some(state) = state else {
        return incomplete(scope, InvariantViolation::MissingPackageState);
    };
    if state.package_key() != *key {
        return incomplete(scope, InvariantViolation::PackageStateMismatch);
    }
    if has_unfinished(&journal, key) {
        return incomplete(scope, InvariantViolation::UnfinishedTransaction);
    }
    let Some((base, slots)) = catalog_views(&catalog, key, &enrolled) else {
        return incomplete(scope, InvariantViolation::CatalogMismatch);
    };
    let Some(active) = active_view(&enrolled, registry.as_ref(), base, &slots) else {
        return incomplete(scope, InvariantViolation::ActiveViewUnproven);
    };
    if !active_slot_metadata_ready(stores, &enrolled, &active) {
        return incomplete(scope, InvariantViolation::ActiveSlotMetadataUnready);
    }
    if !registry_matches_journal(registry.as_ref(), &journal, key) {
        return incomplete(scope, InvariantViolation::RegistryJournalMismatch);
    }

    let durable = DurablePackageFacts::Enrolled {
        lifecycle: state.lifecycle_state(),
        active_slot: active.slot_id().clone(),
    };
    if state.lifecycle_state() != LifecycleState::Normal {
        return facts(scope, durable);
    }
    let Ok(managed) = ManagedPackage::new(
        key.clone(),
        enrolled.identity().clone(),
        enrolled.base_inodes(),
        active,
        state.lifecycle_state(),
    ) else {
        return incomplete(scope, InvariantViolation::ManagedPackageInvalid);
    };
    let Ok(observation) = probe.observe_package(key.package_name(), key.user_id()) else {
        return PackageFacts::new(
            scope,
            durable,
            LivePackageFacts::RecoveryRequired(InvariantViolation::LiveObservationUnavailable),
            GateFacts::NotObserved,
        );
    };
    let support = observation.compatibility().support_level();
    if support == crate::domain::PackageSupportLevel::Blocked {
        return with_live_quarantine(scope, durable, InvariantViolation::PackageSupportBlocked);
    }
    if !policy.is_some_and(|value| value.accepts(observation.identity(), support)) {
        return with_live_quarantine(
            scope,
            durable,
            InvariantViolation::CompatibilityPolicyMismatch,
        );
    }
    let live = match PackageLifecycleGuard::assess(&managed, &observation) {
        GuardDecision::Quarantine => {
            LivePackageFacts::Quarantined(InvariantViolation::IdentityChanged)
        }
        GuardDecision::RecoveryRequired(reason) => {
            LivePackageFacts::RecoveryRequired(map_recovery_reason(reason))
        }
        GuardDecision::RequireSafeUpdateWindow => {
            LivePackageFacts::RecoveryRequired(InvariantViolation::UpdateWindowRequired)
        }
        GuardDecision::AllowBase
        | GuardDecision::AllowSlot
        | GuardDecision::AllowUpdateVerification => LivePackageFacts::Healthy,
    };
    if live != LivePackageFacts::Healthy {
        return PackageFacts::new(scope, durable, live, GateFacts::NotObserved);
    }
    let gate = if probe
        .gate_snapshot(key.package_name(), key.user_id())
        .is_ok()
    {
        GateFacts::Observed
    } else {
        GateFacts::Unavailable
    };
    PackageFacts::new(scope, durable, live, gate)
}

fn facts(scope: EvidenceScope, durable: DurablePackageFacts) -> PackageFacts {
    PackageFacts::new(
        scope,
        durable,
        LivePackageFacts::NotObserved,
        GateFacts::NotObserved,
    )
}

fn incomplete(scope: EvidenceScope, violation: InvariantViolation) -> PackageFacts {
    facts(scope, DurablePackageFacts::Incomplete(violation))
}

fn with_live_quarantine(
    scope: EvidenceScope,
    durable: DurablePackageFacts,
    violation: InvariantViolation,
) -> PackageFacts {
    PackageFacts::new(
        scope,
        durable,
        LivePackageFacts::Quarantined(violation),
        GateFacts::NotObserved,
    )
}

const fn map_recovery_reason(reason: RecoveryReason) -> InvariantViolation {
    match reason {
        RecoveryReason::PersistedLifecycleState | RecoveryReason::PersistedRecoveryState => {
            InvariantViolation::PersistedLifecycleState
        }
        RecoveryReason::PackageManagerInodeDrift => InvariantViolation::PackageManagerInodeDrift,
        RecoveryReason::VisibleViewDrift => InvariantViolation::VisibleViewDrift,
        RecoveryReason::UnexpectedPackageReplacement => {
            InvariantViolation::UnexpectedPackageReplacement
        }
    }
}
