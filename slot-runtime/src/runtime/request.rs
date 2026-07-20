use crate::domain::{BootId, CommitNonce, ManagedPackage, SlotView, TransactionId};

#[doc = "Validated identifiers captured for one switch transaction."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchMetadata {
    transaction_id: TransactionId,
    commit_nonce: CommitNonce,
    boot_id: BootId,
}

impl SwitchMetadata {
    #[doc = "Groups the durable transaction id, commit nonce, and current boot id."]
    pub const fn new(
        transaction_id: TransactionId,
        commit_nonce: CommitNonce,
        boot_id: BootId,
    ) -> Self {
        Self {
            transaction_id,
            commit_nonce,
            boot_id,
        }
    }

    #[doc = "Returns the durable transaction id."]
    pub const fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    #[doc = "Returns the Registry commit nonce."]
    pub const fn commit_nonce(&self) -> &CommitNonce {
        &self.commit_nonce
    }

    #[doc = "Returns the boot id captured for this request."]
    pub const fn boot_id(&self) -> &BootId {
        &self.boot_id
    }
}

#[doc = "One immutable request to switch a managed package to a different CE and DE view."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchRequest {
    managed_package: ManagedPackage,
    target_view: SlotView,
    metadata: SwitchMetadata,
}

impl SwitchRequest {
    #[doc = "Groups the committed package contract, target view, and transaction metadata."]
    pub const fn new(
        managed_package: ManagedPackage,
        target_view: SlotView,
        metadata: SwitchMetadata,
    ) -> Self {
        Self {
            managed_package,
            target_view,
            metadata,
        }
    }

    #[doc = "Returns the committed package contract checked by the lifecycle guard."]
    pub const fn managed_package(&self) -> &ManagedPackage {
        &self.managed_package
    }

    #[doc = "Returns the requested target CE and DE view."]
    pub const fn target_view(&self) -> &SlotView {
        &self.target_view
    }

    #[doc = "Returns the transaction identifiers."]
    pub const fn metadata(&self) -> &SwitchMetadata {
        &self.metadata
    }
}
