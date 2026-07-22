use crate::android::PackageProbe;
use crate::catalog::CatalogEntry;
use crate::domain::{ManagedPackage, PackageKey, SlotId, SlotView};
use crate::journal::{JournalEvent, JournalStep, Transaction, TransactionView};
use crate::lifecycle::{GuardDecision, PackageLifecycleGuard};
use crate::registry::PackageRevision;
use crate::service::{ObservedGateState, PackageSnapshot, PackageState, ServiceError};

use super::stores::ProductionStores;

pub(super) fn load<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
) -> Result<PackageState, ServiceError> {
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
        .list()
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let policy = stores
        .compatibility_policy
        .load(key.package_name())
        .map_err(|_| ServiceError::RecoveryRequired)?;

    if attempt.is_some() {
        return Ok(PackageState::RecoveryRequired);
    }
    let Some(enrolled) = enrollment else {
        return if catalog.is_empty()
            && state.is_none()
            && registry.is_none()
            && policy.is_none()
            && journal_for(&journal, key).next().is_none()
        {
            Ok(PackageState::Absent)
        } else {
            Ok(PackageState::RecoveryRequired)
        };
    };
    let Some(state) = state else {
        return Ok(PackageState::RecoveryRequired);
    };
    if state.package_key() != *key || has_unfinished(&journal, key) {
        return Ok(PackageState::RecoveryRequired);
    }
    let Some((base, slots)) = catalog_views(&catalog, key, &enrolled) else {
        return Ok(PackageState::RecoveryRequired);
    };
    let Some(active) = active_view(&enrolled, registry.as_ref(), base, &slots) else {
        return Ok(PackageState::RecoveryRequired);
    };
    if !registry_matches_journal(registry.as_ref(), &journal) {
        return Ok(PackageState::RecoveryRequired);
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
    let support_level = observation.compatibility().support_level();
    if support_level == crate::domain::PackageSupportLevel::Blocked {
        return Ok(PackageState::Quarantined);
    }
    if !policy.is_some_and(|value| value.accepts(observation.identity(), support_level)) {
        return Ok(PackageState::Quarantined);
    }
    match PackageLifecycleGuard::assess(&managed, &observation) {
        GuardDecision::Quarantine => return Ok(PackageState::Quarantined),
        GuardDecision::RecoveryRequired(_) | GuardDecision::RequireSafeUpdateWindow => {
            return Ok(PackageState::RecoveryRequired);
        }
        GuardDecision::AllowBase
        | GuardDecision::AllowSlot
        | GuardDecision::AllowUpdateVerification => {}
    }
    let gate = probe
        .gate_snapshot(key.package_name(), key.user_id())
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let enabled = matches!(
        gate.enabled_state(),
        crate::domain::PackageEnabledState::Default | crate::domain::PackageEnabledState::Enabled
    );
    Ok(PackageState::Ready(Box::new(PackageSnapshot::new(
        managed,
        slots,
        ObservedGateState::new(enabled, gate.suspended()),
    ))))
}

pub(super) fn catalog_views(
    entries: &[CatalogEntry],
    key: &PackageKey,
    enrolled: &ManagedPackage,
) -> Option<(SlotView, Vec<SlotView>)> {
    if entries.is_empty() || entries.len() > 64 {
        return None;
    }
    let base = entries.iter().find(|entry| entry.slot_id().is_base())?;
    if base.package_key() != key
        || base.inodes() != enrolled.base_inodes()
        || base.enrolled_identity() != enrolled.identity()
    {
        return None;
    }
    let slots = entries
        .iter()
        .filter(|entry| !entry.slot_id().is_base())
        .collect::<Vec<_>>();
    if slots
        .iter()
        .any(|entry| entry.package_key() != key || entry.enrolled_identity() != enrolled.identity())
    {
        return None;
    }
    Some((
        SlotView::new(SlotId::base(), base.inodes()),
        slots
            .into_iter()
            .map(|entry| SlotView::new(entry.slot_id().clone(), entry.inodes()))
            .collect(),
    ))
}

fn active_view(
    enrolled: &ManagedPackage,
    revision: Option<&PackageRevision>,
    base: SlotView,
    slots: &[SlotView],
) -> Option<SlotView> {
    let Some(revision) = revision else {
        return Some(base);
    };
    if revision.user_id() != enrolled.user_id()
        || revision.identity() != enrolled.identity()
        || revision.base_inodes() != enrolled.base_inodes()
    {
        return None;
    }
    if revision.active_slot().is_base() {
        (revision.active_inodes() == base.inodes()).then_some(base)
    } else {
        slots
            .iter()
            .find(|view| {
                view.slot_id() == revision.active_slot()
                    && view.inodes() == revision.active_inodes()
            })
            .cloned()
    }
}

fn journal_for<'a>(
    transactions: &'a [Transaction],
    key: &'a PackageKey,
) -> impl Iterator<Item = &'a Transaction> {
    transactions.iter().filter(|transaction| {
        transaction.spec().package_name() == key.package_name()
            && transaction.spec().user_id() == key.user_id()
    })
}

fn has_unfinished(transactions: &[Transaction], key: &PackageKey) -> bool {
    journal_for(transactions, key).any(|transaction| {
        !matches!(
            transaction.steps().last().map(JournalStep::event),
            Some(JournalEvent::Completed)
        )
    })
}

fn registry_matches_journal(
    revision: Option<&PackageRevision>,
    transactions: &[Transaction],
) -> bool {
    let Some(revision) = revision else {
        return transactions
            .iter()
            .all(|transaction| transaction.view() != TransactionView::CompletedTarget);
    };
    transactions.iter().any(|transaction| {
        transaction.spec().transaction_id() == revision.transaction_id()
            && transaction.view() == TransactionView::CompletedTarget
            && transaction.registry_committed_nonce() == Some(revision.commit_nonce())
            && transaction.spec().target_slot() == revision.active_slot()
            && transaction.spec().target_inodes() == revision.active_inodes()
    })
}
