use serde::{Deserialize, Serialize};

use super::AppIdentity;
use crate::domain::{DataInodes, DomainError, PackageKey, PackageName, SlotId, SlotView, UserId};
use crate::lifecycle::LifecycleState;

#[doc = "Persisted package contract used by lifecycle checks."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ManagedPackageRecord")]
pub struct ManagedPackage {
    package_name: PackageName,
    user_id: UserId,
    identity: AppIdentity,
    base_inodes: DataInodes,
    active_slot: SlotId,
    active_inodes: DataInodes,
    lifecycle_state: LifecycleState,
}

#[derive(Debug, Deserialize)]
struct ManagedPackageRecord {
    package_name: PackageName,
    user_id: UserId,
    identity: AppIdentity,
    base_inodes: DataInodes,
    active_slot: SlotId,
    active_inodes: DataInodes,
    lifecycle_state: LifecycleState,
}

impl TryFrom<ManagedPackageRecord> for ManagedPackage {
    type Error = DomainError;

    fn try_from(value: ManagedPackageRecord) -> Result<Self, Self::Error> {
        Self::new(
            PackageKey::new(value.package_name, value.user_id),
            value.identity,
            value.base_inodes,
            SlotView::new(value.active_slot, value.active_inodes),
            value.lifecycle_state,
        )
    }
}

impl ManagedPackage {
    #[doc = "Constructs a package contract while enforcing base-slot inode rules."]
    pub fn new(
        package: PackageKey,
        identity: AppIdentity,
        base_inodes: DataInodes,
        active_view: SlotView,
        lifecycle_state: LifecycleState,
    ) -> Result<Self, DomainError> {
        let PackageKey {
            package_name,
            user_id,
        } = package;
        let SlotView {
            slot_id: active_slot,
            inodes: active_inodes,
        } = active_view;
        if active_slot.is_base() && active_inodes != base_inodes {
            return Err(DomainError::BaseSlotInodeMismatch);
        }
        if !active_slot.is_base() && active_inodes == base_inodes {
            return Err(DomainError::DuplicateSlotInodes);
        }
        Ok(Self {
            package_name,
            user_id,
            identity,
            base_inodes,
            active_slot,
            active_inodes,
            lifecycle_state,
        })
    }

    #[doc = "Returns a copy with a different persisted lifecycle state."]
    #[must_use]
    pub const fn with_lifecycle_state(mut self, state: LifecycleState) -> Self {
        self.lifecycle_state = state;
        self
    }

    #[doc = "Returns the package name."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the Android user id."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[doc = "Returns the enrolled package identity."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }

    #[doc = "Returns the immutable `PackageManager` inode anchor."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }

    #[doc = "Returns the committed active slot."]
    pub const fn active_slot(&self) -> &SlotId {
        &self.active_slot
    }

    #[doc = "Returns the committed active CE/DE inode pair."]
    pub const fn active_inodes(&self) -> DataInodes {
        self.active_inodes
    }

    #[doc = "Returns the persisted lifecycle state."]
    pub const fn lifecycle_state(&self) -> LifecycleState {
        self.lifecycle_state
    }
}
