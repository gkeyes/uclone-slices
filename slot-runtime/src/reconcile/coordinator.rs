use crate::domain::{GateSnapshot, ManagedPackage, PackageName};
use crate::enrollment::EnrollmentStore;
use crate::journal::{JournalEvent, JournalStep, JournalStore, Transaction};
use crate::registry::RegistryStore;

use super::backend::RecoveryBackend;
use super::error::ReconcileError;
use super::gate::orphan_result;
use super::model::{
    HeldPackage, JournalMetadata, PackageReconcileResult, ReconcileOutcome, ReconcileReason,
    ReconcileReport, RegistryMetadata,
};

#[doc = "Owns the metadata stores and platform backend for two-phase reconciliation."]
#[derive(Debug)]
pub struct Reconciler<B> {
    pub(super) backend: B,
    pub(super) enrollment: EnrollmentStore,
    pub(super) journal: JournalStore,
    pub(super) registry: RegistryStore,
    pub(super) held: Vec<HeldPackage>,
    pub(super) orphaned: Vec<PackageName>,
    boot_started: bool,
}

impl<B: RecoveryBackend> Reconciler<B> {
    #[doc = "Creates a reconciler without reading package or CE state."]
    pub const fn new(
        backend: B,
        enrollment: EnrollmentStore,
        journal: JournalStore,
        registry: RegistryStore,
    ) -> Self {
        Self {
            backend,
            enrollment,
            journal,
            registry,
            held: Vec::new(),
            orphaned: Vec::new(),
            boot_started: false,
        }
    }

    #[doc = "Holds every enrolled or durably leased package without observing or applying CE."]
    pub fn early_boot(&mut self) -> Result<ReconcileReport, ReconcileError> {
        if self.boot_started {
            let mut results: Vec<_> = self.held.iter().map(early_result).collect();
            results.extend(self.orphaned.iter().cloned().map(orphan_result));
            return Ok(ReconcileReport::new(results));
        }
        let scan = self.enrollment.package_names()?;
        let mut package_names = scan.package_names().to_vec();
        package_names.extend(
            self.backend
                .leased_packages()
                .map_err(ReconcileError::LeaseDiscovery)?,
        );
        package_names.sort();
        package_names.dedup();
        let snapshots = self.emergency_gate_discovered(&package_names)?;
        let packages = self.enrollment.list()?;
        let transactions = self.journal.list().ok();
        self.orphaned = snapshots
            .keys()
            .filter(|name| {
                !packages
                    .iter()
                    .any(|package| package.package_name() == *name)
            })
            .cloned()
            .collect();
        let mut held_packages = Vec::with_capacity(packages.len());
        for managed in packages {
            let snapshot = snapshots
                .get(managed.package_name())
                .copied()
                .ok_or_else(|| {
                    ReconcileError::MissingEmergencySnapshot(managed.package_name().clone())
                })?;
            self.backend
                .confirm_emergency_gate(&managed)
                .map_err(|source| ReconcileError::ConfirmEmergencyGate {
                    package: managed.package_name().clone(),
                    source,
                })?;
            held_packages.push(self.hold_package(managed, transactions.as_deref(), snapshot));
        }
        self.held = held_packages;
        self.boot_started = true;
        let mut results: Vec<_> = self.held.iter().map(early_result).collect();
        results.extend(self.orphaned.iter().cloned().map(orphan_result));
        Ok(ReconcileReport::new(results))
    }

    #[doc = "Restores and proves committed views after user0 unlock, then restores exact gates."]
    pub fn reconcile_unlocked(&mut self) -> Result<ReconcileReport, ReconcileError> {
        if !self.boot_started {
            return Err(ReconcileError::EarlyBootRequired);
        }
        self.rehold_packages()?;
        self.rehold_orphans()?;
        let unlocked = self
            .backend
            .user0_unlocked()
            .map_err(ReconcileError::UnlockState)?;
        if !unlocked {
            let held_packages = self.held.clone();
            let mut results = Vec::with_capacity(held_packages.len());
            for held in &held_packages {
                let outcome = if let Some(reason) = initial_reason(held) {
                    self.require_recovery(held, reason)?
                } else {
                    ReconcileOutcome::Locked
                };
                results.push(package_result(held, outcome));
            }
            results.extend(self.orphaned.iter().cloned().map(orphan_result));
            return Ok(ReconcileReport::new(results));
        }

        let mut packages = core::mem::take(&mut self.held).into_iter();
        let mut remaining = Vec::new();
        let mut results = Vec::with_capacity(packages.len());
        while let Some(held) = packages.next() {
            match self.reconcile_package(&held) {
                Ok(outcome) => {
                    results.push(package_result(&held, outcome.clone()));
                    if !outcome.releases_gate() {
                        remaining.push(held);
                    }
                }
                Err(error) => {
                    remaining.push(held);
                    remaining.extend(packages);
                    self.held = remaining;
                    return Err(error);
                }
            }
        }
        self.held = remaining;
        results.extend(self.orphaned.iter().cloned().map(orphan_result));
        Ok(ReconcileReport::new(results))
    }

    #[doc = "Returns the platform backend for evidence inspection or later reuse."]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    fn hold_package(
        &self,
        managed: ManagedPackage,
        transactions: Option<&[Transaction]>,
        emergency_snapshot: GateSnapshot,
    ) -> HeldPackage {
        let (journal, snapshot) = journal_metadata(&managed, transactions, emergency_snapshot);
        let registry = self
            .registry
            .latest(managed.package_name())
            .map_or(RegistryMetadata::Invalid, |revision| {
                RegistryMetadata::Valid(Box::new(revision))
            });
        HeldPackage {
            managed,
            snapshot,
            journal,
            registry,
            gate_proved: true,
        }
    }
}

fn journal_metadata(
    managed: &ManagedPackage,
    transactions: Option<&[Transaction]>,
    emergency_snapshot: GateSnapshot,
) -> (JournalMetadata, Option<crate::domain::GateSnapshot>) {
    let Some(transactions) = transactions else {
        return (JournalMetadata::Invalid, None);
    };
    let unfinished: Vec<&Transaction> = transactions
        .iter()
        .filter(|transaction| {
            transaction.spec().package_name() == managed.package_name()
                && !matches!(
                    transaction.steps().last().map(JournalStep::event),
                    Some(JournalEvent::Completed)
                )
        })
        .collect();
    match unfinished.as_slice() {
        [] => (JournalMetadata::Clean, Some(emergency_snapshot)),
        [transaction] => (
            JournalMetadata::Transaction(transaction.spec().transaction_id().clone()),
            (transaction.spec().gate_snapshot() == emergency_snapshot)
                .then_some(emergency_snapshot),
        ),
        _ => (JournalMetadata::Ambiguous, None),
    }
}

fn early_result(held: &HeldPackage) -> PackageReconcileResult {
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
