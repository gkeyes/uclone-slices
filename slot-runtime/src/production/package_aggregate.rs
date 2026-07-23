use crate::android::PackageProbe;
use crate::domain::{
    DurablePackageFacts, EvidenceScope, GateFacts, InvariantViolation, LivePackageFacts,
    ManagedPackage, PackageAggregate, PackageFacts, PackageKey, ReadyPackageEvidence,
};
use crate::lifecycle::{GuardDecision, LifecycleState, PackageLifecycleGuard, RecoveryReason};
use crate::service::ServiceError;

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
) -> Result<PackageAggregate, ServiceError> {
    load_facts(stores, probe, key, scope).map(PackageAggregate::resolve)
}

fn load_facts<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
    scope: EvidenceScope,
) -> Result<PackageFacts, ServiceError> {
    let attempt = stores
        .attempts
        .load(key)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let enrollment = stores
        .enrollment
        .load(key.package_name())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let catalog = stores
        .catalog
        .list(key)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let state = stores
        .package_state
        .latest(key)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let registry = stores
        .registry
        .latest(key.package_name())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let journal = stores
        .journal
        .list_for_package(key)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let policy = stores
        .compatibility_policy
        .load(key.package_name())
        .map_err(|_| ServiceError::RecoveryRequired)?;

    if attempt.is_some() {
        return Ok(facts(scope, DurablePackageFacts::EnrollmentAttempt));
    }
    let Some(enrolled) = enrollment else {
        let clean = catalog.is_empty()
            && state.is_none()
            && registry.is_none()
            && policy.is_none()
            && super::state::journal_for(&journal, key).next().is_none();
        return Ok(facts(
            scope,
            if clean {
                DurablePackageFacts::Absent
            } else {
                DurablePackageFacts::Orphaned
            },
        ));
    };
    let Some(state) = state else {
        return Ok(incomplete(scope, InvariantViolation::MissingPackageState));
    };
    if state.package_key() != *key {
        return Ok(incomplete(scope, InvariantViolation::PackageStateMismatch));
    }
    if has_unfinished(&journal, key) {
        return Ok(incomplete(scope, InvariantViolation::UnfinishedTransaction));
    }
    let Some((base, slots)) = catalog_views(&catalog, key, &enrolled) else {
        return Ok(incomplete(scope, InvariantViolation::CatalogMismatch));
    };
    let Some(active) = active_view(&enrolled, registry.as_ref(), base, &slots) else {
        return Ok(incomplete(scope, InvariantViolation::ActiveViewUnproven));
    };
    if !active_slot_metadata_ready(stores, &enrolled, &active) {
        return Ok(incomplete(
            scope,
            InvariantViolation::ActiveSlotMetadataUnready,
        ));
    }
    if !registry_matches_journal(registry.as_ref(), &journal, key) {
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
