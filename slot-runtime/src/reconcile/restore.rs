use crate::domain::{ManagedPackage, PackageObservation, SlotView, TransactionId};
use crate::journal::JournalEvent;

use super::assessment::initial_reason;
use super::backend::RecoveryBackend;
use super::coordinator::Reconciler;
use super::error::ReconcileError;
use super::model::{HeldPackage, JournalMetadata, ReconcileOutcome, ReconcileReason};

impl<B: RecoveryBackend> Reconciler<B> {
    pub(super) fn reconcile_package(
        &mut self,
        held: &HeldPackage,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        if let Some(reason) = initial_reason(held) {
            return self.require_recovery(held, reason);
        }
        if self
            .backend
            .acquire_gate(&held.managed)
            .and_then(|()| self.backend.verify_gate_held(&held.managed))
            .is_err()
        {
            return self.require_recovery(held, ReconcileReason::GateHoldFailed);
        }
        let Ok(observation) = self.backend.observe_package(&held.managed) else {
            return self.require_recovery(held, ReconcileReason::PackageStateDrift);
        };
        match live_package_state(&held.managed, &observation) {
            LivePackageState::Healthy => {}
            LivePackageState::Quarantined => return self.quarantine(held),
            LivePackageState::Drifted => {
                return self.require_recovery(held, ReconcileReason::PackageStateDrift);
            }
        }
        match &held.journal {
            JournalMetadata::Clean => self.restore_committed(held),
            JournalMetadata::Transaction(transaction_id) => {
                self.reconcile_transaction(held, transaction_id)
            }
            JournalMetadata::Ambiguous => {
                self.require_recovery(held, ReconcileReason::AmbiguousTransactions)
            }
            JournalMetadata::Invalid => {
                self.require_recovery(held, ReconcileReason::JournalMetadata)
            }
        }
    }

    pub(super) fn require_recovery(
        &mut self,
        held: &HeldPackage,
        reason: ReconcileReason,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        self.contain(&held.managed)?;
        if let Some(transaction_id) = held.transaction_id() {
            self.record_recovery(transaction_id, reason.code())?;
        }
        Ok(ReconcileOutcome::RecoveryRequired(reason))
    }

    pub(super) fn restore_view(&mut self, managed: &ManagedPackage, view: &SlotView) -> bool {
        if view.slot_id().is_base() {
            self.backend.verify_native_base(managed).is_ok()
        } else {
            self.backend
                .apply_slot_view(managed, view)
                .and_then(|()| self.backend.verify_slot_view(managed, view))
                .is_ok()
        }
    }

    pub(super) fn restore_gate(&mut self, held: &HeldPackage) -> bool {
        let Some(snapshot) = held.snapshot else {
            return false;
        };
        self.backend.restore_gate(&held.managed, snapshot).is_ok()
    }

    pub(super) fn contain(&mut self, managed: &ManagedPackage) -> Result<(), ReconcileError> {
        self.backend
            .emergency_gate(managed.package_name())
            .and_then(|_| self.backend.confirm_emergency_gate(managed))
            .map_err(|source| ReconcileError::Containment {
                package: managed.package_name().clone(),
                source,
            })
    }

    pub(super) fn retire_gate_or_recover(
        &mut self,
        held: &HeldPackage,
    ) -> Result<Option<ReconcileOutcome>, ReconcileError> {
        if self.backend.retire_gate_lease(&held.managed).is_ok() {
            return Ok(None);
        }
        self.contain(&held.managed)?;
        Ok(Some(ReconcileOutcome::RecoveryRequired(
            ReconcileReason::GateLeaseRetirement,
        )))
    }

    pub(super) fn record_recovery(
        &self,
        transaction_id: &TransactionId,
        reason: &str,
    ) -> Result<(), ReconcileError> {
        self.journal
            .append(
                transaction_id,
                JournalEvent::RecoveryRequired {
                    reason: reason.to_owned(),
                },
            )
            .map(|_| ())
            .map_err(|source| ReconcileError::RecoveryMarker {
                transaction_id: transaction_id.clone(),
                source,
            })
    }

    fn quarantine(&mut self, held: &HeldPackage) -> Result<ReconcileOutcome, ReconcileError> {
        self.contain(&held.managed)?;
        if let Some(transaction_id) = held.transaction_id() {
            self.record_recovery(transaction_id, "identity_mismatch")?;
        }
        Ok(ReconcileOutcome::Quarantined)
    }

    pub(super) fn prove_release_ready(
        &mut self,
        managed: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), ReconcileReason> {
        let observation = self
            .backend
            .observe_package(managed)
            .map_err(|_| ReconcileReason::PackageStateDrift)?;
        if live_package_state(managed, &observation) != LivePackageState::Healthy {
            return Err(ReconcileReason::PackageStateDrift);
        }
        if view.slot_id().is_base() {
            self.backend
                .verify_native_base(managed)
                .map_err(|_| ReconcileReason::ViewRestoreFailed)
        } else {
            self.backend
                .verify_slot_view(managed, view)
                .map_err(|_| ReconcileReason::ViewRestoreFailed)
        }
    }
}

fn live_package_state(managed: &ManagedPackage, observed: &PackageObservation) -> LivePackageState {
    let expected = managed.identity();
    let actual = observed.identity();
    if expected.uid() != actual.uid() || expected.signature_sha256() != actual.signature_sha256() {
        return LivePackageState::Quarantined;
    }
    let identity_metadata_drifted = expected != actual;
    let package_manager_base_drifted = observed.package_manager_inodes() != managed.base_inodes();
    if identity_metadata_drifted || package_manager_base_drifted || observed.pending_install() {
        LivePackageState::Drifted
    } else {
        LivePackageState::Healthy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LivePackageState {
    Healthy,
    Drifted,
    Quarantined,
}
