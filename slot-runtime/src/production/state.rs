use crate::android::PackageProbe;
use crate::catalog::CatalogEntry;
use crate::domain::{
    AggregateState, EvidenceScope, ManagedPackage, PackageEnabledState, PackageKey, SlotId,
    SlotView,
};
use crate::journal::{JournalEvent, JournalStep, Transaction, TransactionView};
use crate::registry::PackageRevision;
use crate::service::{ObservedGateState, PackageSnapshot, PackageState, ServiceError};
use crate::slot_metadata::SlotRecordState;

use super::stores::ProductionStores;

pub(super) fn load<Q: PackageProbe>(
    stores: &ProductionStores,
    probe: &mut Q,
    key: &PackageKey,
) -> Result<PackageState, ServiceError> {
    let aggregate = super::package_aggregate::resolve(stores, probe, key, EvidenceScope::Unlocked)?;
    match aggregate.state() {
        AggregateState::Absent => Ok(PackageState::Absent),
        AggregateState::RecoveryRequired => Ok(PackageState::RecoveryRequired),
        AggregateState::Quarantined => Ok(PackageState::Quarantined),
        AggregateState::Ready => {
            let evidence = aggregate
                .into_ready_evidence()
                .ok_or(ServiceError::RecoveryRequired)?;
            let (managed, slots, gate) = evidence.into_parts();
            let enabled = matches!(
                gate.enabled_state(),
                PackageEnabledState::Default | PackageEnabledState::Enabled
            );
            Ok(PackageState::Ready(Box::new(PackageSnapshot::new(
                managed,
                slots,
                ObservedGateState::new(enabled, gate.suspended()),
            ))))
        }
    }
}

pub(super) fn active_slot_metadata_ready(
    stores: &ProductionStores,
    enrolled: &ManagedPackage,
    active: &SlotView,
) -> bool {
    active.slot_id().is_base()
        || matches!(
            stores
                .slot_metadata
                .latest(enrolled.package_name(), active.slot_id()),
            Ok(Some(record)) if record.state() == SlotRecordState::Ready
        )
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

pub(super) fn active_view(
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

pub(super) fn journal_for<'a>(
    transactions: &'a [Transaction],
    key: &'a PackageKey,
) -> impl Iterator<Item = &'a Transaction> {
    transactions.iter().filter(|transaction| {
        transaction.spec().package_name() == key.package_name()
            && transaction.spec().user_id() == key.user_id()
    })
}

pub(super) fn has_unfinished(transactions: &[Transaction], key: &PackageKey) -> bool {
    journal_for(transactions, key).any(|transaction| {
        !matches!(
            transaction.steps().last().map(JournalStep::event),
            Some(JournalEvent::Completed)
        )
    })
}

pub(super) fn registry_matches_journal(
    revision: Option<&PackageRevision>,
    transactions: &[Transaction],
    key: &PackageKey,
) -> bool {
    let Some(revision) = revision else {
        return journal_for(transactions, key)
            .all(|transaction| transaction.view() != TransactionView::CompletedTarget);
    };
    journal_for(transactions, key).any(|transaction| {
        transaction.spec().transaction_id() == revision.transaction_id()
            && transaction.view() == TransactionView::CompletedTarget
            && transaction.registry_committed_nonce() == Some(revision.commit_nonce())
            && transaction.spec().target_slot() == revision.active_slot()
            && transaction.spec().target_inodes() == revision.active_inodes()
    })
}
