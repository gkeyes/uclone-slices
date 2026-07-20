use serde::{Deserialize, Serialize};

use super::RegistryError;
use super::chain;
use crate::domain::{
    AppIdentity, CommitNonce, DataInodes, PackageName, SlotId, TransactionId, UserId,
};
use crate::journal::TransactionSpec;
use crate::lifecycle::LifecycleState;

pub(super) const SCHEMA_VERSION: u32 = 2;

#[doc = "One immutable Registry revision published at the transaction commit point."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRevision {
    pub(super) schema_version: u32,
    pub(super) generation: u64,
    pub(super) package_name: PackageName,
    pub(super) user_id: UserId,
    pub(super) identity: AppIdentity,
    pub(super) lifecycle_state: LifecycleState,
    pub(super) base_inodes: DataInodes,
    pub(super) previous_slot: SlotId,
    pub(super) previous_inodes: DataInodes,
    pub(super) active_slot: SlotId,
    pub(super) active_inodes: DataInodes,
    pub(super) transaction_id: TransactionId,
    pub(super) commit_nonce: CommitNonce,
    pub(super) previous_sha256: Option<String>,
    pub(super) sha256: String,
}

impl PackageRevision {
    #[doc = "Builds an unpublished revision from a verified transaction target."]
    pub fn committed(
        spec: &TransactionSpec,
        base_inodes: DataInodes,
        commit_nonce: CommitNonce,
    ) -> Result<Self, RegistryError> {
        if spec.base_inodes() != base_inodes {
            return Err(RegistryError::InvalidRevision(
                "transaction base anchor differs from Registry base anchor".to_owned(),
            ));
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            generation: 0,
            package_name: spec.package_name().clone(),
            user_id: spec.user_id(),
            identity: spec.identity().clone(),
            lifecycle_state: spec.lifecycle_state(),
            base_inodes,
            previous_slot: spec.previous_slot().clone(),
            previous_inodes: spec.previous_inodes(),
            active_slot: spec.target_slot().clone(),
            active_inodes: spec.target_inodes(),
            transaction_id: spec.transaction_id().clone(),
            commit_nonce,
            previous_sha256: None,
            sha256: String::new(),
        })
    }

    #[doc = "Returns the committed package."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the immutable base inode anchor."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }

    #[doc = "Returns the Android user id for this Registry stream."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[doc = "Returns the package identity protected by this Registry stream."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }

    #[doc = "Returns the package lifecycle state at this commit point."]
    pub const fn lifecycle_state(&self) -> LifecycleState {
        self.lifecycle_state
    }

    #[doc = "Returns the committed active slot."]
    pub const fn active_slot(&self) -> &SlotId {
        &self.active_slot
    }

    #[doc = "Returns the committed active inode pair."]
    pub const fn active_inodes(&self) -> DataInodes {
        self.active_inodes
    }

    #[doc = "Returns the transaction that produced this revision."]
    pub const fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    #[doc = "Returns the transaction commit nonce."]
    pub const fn commit_nonce(&self) -> &CommitNonce {
        &self.commit_nonce
    }

    pub(super) fn publish_after(&self, previous: Option<&Self>) -> Result<Self, RegistryError> {
        chain::publish(self, previous)
    }

    pub(super) fn verify(&self, previous: Option<&Self>) -> Result<(), RegistryError> {
        chain::verify(self, previous)
    }
}
