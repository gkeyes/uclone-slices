use crate::domain::{ManagedPackage, PackageKey, SlotId, SlotView};
use crate::registry::PackageRevision;

use super::backend::RecoveryBackend;
use super::coordinator::Reconciler;
use super::error::ReconcileError;
use super::model::{HeldPackage, ReconcileOutcome, ReconcileReason, RegistryMetadata};

impl<B: RecoveryBackend> Reconciler<B> {
    pub(super) fn restore_committed(
        &mut self,
        held: &HeldPackage,
    ) -> Result<ReconcileOutcome, ReconcileError> {
        let RegistryMetadata::Valid(revision) = &held.registry else {
            return self.require_recovery(held, ReconcileReason::RegistryMetadata);
        };
        let Some(desired) = committed_package(&held.managed, revision.as_ref().as_ref()) else {
            return self.require_recovery(held, ReconcileReason::RegistryMetadata);
        };
        let view = SlotView::new(desired.active_slot().clone(), desired.active_inodes());
        if !self.restore_committed_view(held, &desired, &view) {
            return self.require_recovery(held, ReconcileReason::ViewRestoreFailed);
        }
        if let Err(reason) = self.prove_release_ready(&desired, &view) {
            return self.require_recovery(held, reason);
        }
        if !self.restore_gate(held, &desired) {
            return self.require_recovery(held, ReconcileReason::GateRestoreFailed);
        }
        if let Some(outcome) = self.retire_gate_or_recover(held)? {
            return Ok(outcome);
        }
        if view.slot_id().is_base() {
            Ok(ReconcileOutcome::RestoredBase)
        } else {
            Ok(ReconcileOutcome::RestoredSlot(view.slot_id().clone()))
        }
    }

    fn restore_committed_view(
        &mut self,
        held: &HeldPackage,
        desired: &ManagedPackage,
        view: &SlotView,
    ) -> bool {
        if self.verify_view(desired, view) {
            return true;
        }
        let registered = SlotView::new(
            held.managed.active_slot().clone(),
            held.managed.active_inodes(),
        );
        if self.verify_view(&held.managed, &registered) {
            return self.apply_and_verify(&held.managed, desired, view);
        }
        let Some(base) = base_active_package(&held.managed) else {
            return false;
        };
        if self.backend.verify_native_base(&base).is_err() {
            return false;
        }
        self.apply_and_verify(&base, desired, view)
    }

    fn verify_view(&mut self, managed: &ManagedPackage, view: &SlotView) -> bool {
        if view.slot_id().is_base() {
            self.backend.verify_native_base(managed).is_ok()
        } else {
            self.backend.verify_slot_view(managed, view).is_ok()
        }
    }

    fn apply_and_verify(
        &mut self,
        current: &ManagedPackage,
        desired: &ManagedPackage,
        view: &SlotView,
    ) -> bool {
        self.backend
            .apply_slot_view(current, view)
            .and_then(|()| self.backend.verify_slot_view(desired, view))
            .is_ok()
    }
}

fn committed_package(
    enrolled: &ManagedPackage,
    revision: Option<&PackageRevision>,
) -> Option<ManagedPackage> {
    let Some(revision) = revision else {
        return Some(enrolled.clone());
    };
    if revision.package_name() != enrolled.package_name()
        || revision.user_id() != enrolled.user_id()
        || revision.identity() != enrolled.identity()
        || revision.lifecycle_state() != enrolled.lifecycle_state()
        || revision.base_inodes() != enrolled.base_inodes()
    {
        return None;
    }
    ManagedPackage::new(
        PackageKey::new(enrolled.package_name().clone(), enrolled.user_id()),
        enrolled.identity().clone(),
        enrolled.base_inodes(),
        SlotView::new(revision.active_slot().clone(), revision.active_inodes()),
        revision.lifecycle_state(),
    )
    .ok()
}

fn base_active_package(enrolled: &ManagedPackage) -> Option<ManagedPackage> {
    ManagedPackage::new(
        PackageKey::new(enrolled.package_name().clone(), enrolled.user_id()),
        enrolled.identity().clone(),
        enrolled.base_inodes(),
        SlotView::new(SlotId::base(), enrolled.base_inodes()),
        enrolled.lifecycle_state(),
    )
    .ok()
}
