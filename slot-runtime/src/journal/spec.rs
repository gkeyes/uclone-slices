use serde::{Deserialize, Serialize};

use super::JournalError;
use crate::domain::{
    AppIdentity, BootId, DataInodes, GateSnapshot, ManagedPackage, PackageName, SlotId, SlotView,
    TransactionId, UserId,
};
use crate::lifecycle::LifecycleState;

#[doc = "A complete immutable description of one requested slot-view transaction."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TransactionSpecRecord")]
pub struct TransactionSpec {
    transaction_id: TransactionId,
    managed_package: ManagedPackage,
    target_view: SlotView,
    gate_snapshot: GateSnapshot,
    boot_id: BootId,
}

#[derive(Debug, Deserialize)]
struct TransactionSpecRecord {
    transaction_id: TransactionId,
    managed_package: ManagedPackage,
    target_view: SlotView,
    gate_snapshot: GateSnapshot,
    boot_id: BootId,
}

impl TryFrom<TransactionSpecRecord> for TransactionSpec {
    type Error = JournalError;

    fn try_from(value: TransactionSpecRecord) -> Result<Self, Self::Error> {
        let spec = Self {
            transaction_id: value.transaction_id,
            managed_package: value.managed_package,
            target_view: value.target_view,
            gate_snapshot: value.gate_snapshot,
            boot_id: value.boot_id,
        };
        spec.validate()?;
        Ok(spec)
    }
}

impl TransactionSpec {
    #[doc = "Constructs a transaction from a committed package view and target view."]
    pub fn new(
        transaction_id: TransactionId,
        managed_package: ManagedPackage,
        target_view: SlotView,
        gate_snapshot: GateSnapshot,
        boot_id: &str,
    ) -> Result<Self, JournalError> {
        let spec = Self {
            transaction_id,
            managed_package,
            target_view,
            gate_snapshot,
            boot_id: BootId::parse(boot_id)?,
        };
        spec.validate()?;
        Ok(spec)
    }

    #[doc = "Returns the transaction identifier."]
    pub const fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    #[doc = "Returns the target package name."]
    pub const fn package_name(&self) -> &PackageName {
        self.managed_package.package_name()
    }

    #[doc = "Returns the target Android user id."]
    pub const fn user_id(&self) -> UserId {
        self.managed_package.user_id()
    }

    #[doc = "Returns the immutable Android-owned base inode anchor."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.managed_package.base_inodes()
    }

    #[doc = "Returns the package identity observed before the transaction."]
    pub const fn identity(&self) -> &AppIdentity {
        self.managed_package.identity()
    }

    #[doc = "Returns the lifecycle state observed before the transaction."]
    pub const fn lifecycle_state(&self) -> LifecycleState {
        self.managed_package.lifecycle_state()
    }

    #[doc = "Returns the previously committed package contract."]
    pub const fn managed_package(&self) -> &ManagedPackage {
        &self.managed_package
    }

    #[doc = "Returns the previously committed slot."]
    pub const fn previous_slot(&self) -> &SlotId {
        self.managed_package.active_slot()
    }

    #[doc = "Returns the requested target slot."]
    pub const fn target_slot(&self) -> &SlotId {
        self.target_view.slot_id()
    }

    #[doc = "Returns the previously committed CE/DE inodes."]
    pub const fn previous_inodes(&self) -> DataInodes {
        self.managed_package.active_inodes()
    }

    #[doc = "Returns the target slot CE/DE inodes."]
    pub const fn target_inodes(&self) -> DataInodes {
        self.target_view.inodes()
    }

    #[doc = "Returns the package state that must be restored after gate release."]
    pub const fn gate_snapshot(&self) -> GateSnapshot {
        self.gate_snapshot
    }

    #[doc = "Returns the boot id captured before any side effect."]
    pub fn boot_id(&self) -> &str {
        self.boot_id.as_str()
    }

    pub(super) fn validate(&self) -> Result<(), JournalError> {
        if self.previous_slot() == self.target_slot() {
            return Err(JournalError::InvalidSpec(
                "previous and target slots must differ".to_owned(),
            ));
        }
        if !view_matches_base_rule(&self.target_view, self.base_inodes()) {
            return Err(JournalError::InvalidSpec(
                "target view does not preserve the immutable base inode anchor".to_owned(),
            ));
        }
        if self.previous_inodes() == self.target_inodes() {
            return Err(JournalError::InvalidSpec(
                "distinct slots must not share the same inode pair".to_owned(),
            ));
        }
        Ok(())
    }
}

fn view_matches_base_rule(view: &SlotView, base_inodes: DataInodes) -> bool {
    view.slot_id().is_base() == (view.inodes() == base_inodes)
}
