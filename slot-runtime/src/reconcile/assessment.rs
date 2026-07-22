use super::model::{
    HeldPackage, JournalMetadata, PackageReconcileResult, ReconcileOutcome, ReconcileReason,
    RegistryMetadata,
};

pub(super) fn early_result(held: &HeldPackage) -> PackageReconcileResult {
    let outcome =
        initial_reason(held).map_or(ReconcileOutcome::Held, ReconcileOutcome::RecoveryRequired);
    package_result(held, outcome)
}

pub(super) const fn initial_reason(held: &HeldPackage) -> Option<ReconcileReason> {
    if !held.gate_proved {
        return Some(ReconcileReason::GateHoldFailed);
    }
    match held.journal {
        JournalMetadata::Invalid => return Some(ReconcileReason::JournalMetadata),
        JournalMetadata::Ambiguous => return Some(ReconcileReason::AmbiguousTransactions),
        JournalMetadata::Clean | JournalMetadata::Transaction(_) => {}
    }
    if matches!(held.registry, RegistryMetadata::Invalid) {
        return Some(ReconcileReason::RegistryMetadata);
    }
    if held.snapshot.is_none() {
        return Some(ReconcileReason::MissingGateSnapshot);
    }
    None
}

pub(super) fn package_result(
    held: &HeldPackage,
    outcome: ReconcileOutcome,
) -> PackageReconcileResult {
    PackageReconcileResult::new(
        held.managed.package_name().clone(),
        held.transaction_id().cloned(),
        outcome,
    )
}
