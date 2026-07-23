use crate::android::PackageProbe;
use crate::catalog::CatalogEntry;
use crate::compatibility_policy::CompatibilityPolicy;
use crate::domain::{
    DurablePackageFacts, EvidenceScope, GateFacts, InvariantViolation, LivePackageFacts,
    ManagedPackage, PackageAggregate, PackageFacts, PackageKey, ReadyPackageEvidence, SlotView,
};
use crate::enrollment_attempt::EnrollmentAttempt;
use crate::journal::Transaction;
use crate::lifecycle::{GuardDecision, LifecycleState, PackageLifecycleGuard, RecoveryReason};
use crate::package_state::PackageStateRevision;
use crate::registry::PackageRevision;
use crate::service::ServiceError;

use super::state::{
    active_slot_metadata_ready, active_view, catalog_views, has_unfinished,
    registry_matches_journal,
};
use super::stores::ProductionStores;

struct PersistedFacts {
    attempt: Option<EnrollmentAttempt>,
    enrollment: Option<ManagedPackage>,
    catalog: Vec<CatalogEntry>,
    state: Option<PackageStateRevision>,
    registry: Option<PackageRevision>,
    journal: Vec<Transaction>,
    policy: Option<CompatibilityPolicy>,
}

type LiveFactsContext<'a> = (
    EvidenceScope,
    Option<&'a CompatibilityPolicy>,
    &'a PackageStateRevision,
    DurablePackageFacts,
);

pub(super) fn resolve<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
    scope: EvidenceScope,
) -> Result<PackageAggregate, ServiceError> {
    load_facts(stores, probe, key, scope).map(PackageAggregate::resolve)
}

fn load_facts<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
    scope: EvidenceScope,
) -> Result<PackageFacts, ServiceError> {
    let persisted = load_persisted_facts(stores, key)?;
    if persisted.attempt.is_some() {
        return Ok(facts(scope, DurablePackageFacts::EnrollmentAttempt));
    }
    let Some(enrolled) = persisted.enrollment.as_ref() else {
        let clean = persisted.catalog.is_empty()
            && persisted.state.is_none()
            && persisted.registry.is_none()
            && persisted.policy.is_none()
            && super::state::journal_for(&persisted.journal, key)
                .next()
                .is_none();
        return Ok(facts(
            scope,
            if clean {
                DurablePackageFacts::Absent
            } else {
                DurablePackageFacts::Orphaned
            },
        ));
    };
    let Some(state) = persisted.state.as_ref() else {
        return Ok(incomplete(scope, InvariantViolation::MissingPackageState));
    };
    if state.package_key() != *key {
        return Ok(incomplete(scope, InvariantViolation::PackageStateMismatch));
    }
    if has_unfinished(&persisted.journal, key) {
        return Ok(incomplete(scope, InvariantViolation::UnfinishedTransaction));
    }
    let Some((base, slots)) = catalog_views(&persisted.catalog, key, enrolled) else {
        return Ok(incomplete(scope, InvariantViolation::CatalogMismatch));
    };
    let Some(active) = active_view(enrolled, persisted.registry.as_ref(), base, &slots) else {
        return Ok(incomplete(scope, InvariantViolation::ActiveViewUnproven));
    };
    if !active_slot_metadata_ready(stores, enrolled, &active) {
        return Ok(incomplete(
            scope,
            InvariantViolation::ActiveSlotMetadataUnready,
        ));
    }
    if !registry_matches_journal(persisted.registry.as_ref(), &persisted.journal, key) {
        return Ok(incomplete(
            scope,
            InvariantViolation::RegistryJournalMismatch,
        ));
    }

    let durable = DurablePackageFacts::Enrolled {
        lifecycle: state.lifecycle_state(),
        active_slot: active.slot_id().clone(),
    };
    if state.lifecycle_state() != LifecycleState::Normal {
        return Ok(facts(scope, durable));
    }
    let managed = ManagedPackage::new(
        key.clone(),
        enrolled.identity().clone(),
        enrolled.base_inodes(),
        active,
        state.lifecycle_state(),
    )
    .map_err(|_| ServiceError::RecoveryRequired)?;
    load_live_facts(
        probe,
        key,
        (scope, persisted.policy.as_ref(), state, durable),
        managed,
        slots,
    )
}

fn load_persisted_facts(
    stores: &ProductionStores,
    key: &PackageKey,
) -> Result<PersistedFacts, ServiceError> {
    Ok(PersistedFacts {
        attempt: map_store_error(stores.attempts.load(key))?,
        enrollment: map_store_error(stores.enrollment.load(key.package_name()))?,
        catalog: map_store_error(stores.catalog.list(key))?,
        state: map_store_error(stores.package_state.latest(key))?,
        registry: map_store_error(stores.registry.latest(key.package_name()))?,
        journal: map_store_error(stores.journal.list_for_package(key))?,
        policy: map_store_error(stores.compatibility_policy.load(key.package_name()))?,
    })
}

fn map_store_error<T, E>(result: Result<T, E>) -> Result<T, ServiceError> {
    result.map_err(|_| ServiceError::RecoveryRequired)
}

fn load_live_facts<Q: PackageProbe>(
    probe: &mut Q,
    key: &PackageKey,
    context: LiveFactsContext<'_>,
    managed: ManagedPackage,
    slots: Vec<SlotView>,
) -> Result<PackageFacts, ServiceError> {
    let (scope, policy, state, durable) = context;
    let observation = probe
        .observe_package(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let support = observation.compatibility().support_level();
    if support == crate::domain::PackageSupportLevel::Blocked {
        return Ok(with_live_quarantine(
            scope,
            durable,
            InvariantViolation::PackageSupportBlocked,
        ));
    }
    if !policy.is_some_and(|value| value.accepts(observation.identity(), support)) {
        return Ok(with_live_quarantine(
            scope,
            durable,
            InvariantViolation::CompatibilityPolicyMismatch,
        ));
    }
    let live = match PackageLifecycleGuard::assess_with_accepted_artifact(
        &managed,
        &observation,
        state.accepted_artifact(),
    ) {
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
        return Ok(PackageFacts::new(
            scope,
            durable,
            live,
            GateFacts::NotObserved,
        ));
    }
    let gate = probe
        .gate_snapshot(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    Ok(PackageFacts::with_ready_evidence(
        scope,
        durable,
        live,
        GateFacts::Observed,
        ReadyPackageEvidence::new(managed, slots, gate),
    ))
}

const fn facts(scope: EvidenceScope, durable: DurablePackageFacts) -> PackageFacts {
    PackageFacts::new(
        scope,
        durable,
        LivePackageFacts::NotObserved,
        GateFacts::NotObserved,
    )
}

const fn incomplete(scope: EvidenceScope, violation: InvariantViolation) -> PackageFacts {
    facts(scope, DurablePackageFacts::Incomplete(violation))
}

const fn with_live_quarantine(
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
