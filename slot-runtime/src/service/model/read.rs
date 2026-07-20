use crate::domain::{AppIdentity, DataInodes, PackageCompatibility, PackageName, SlotId};
use crate::lifecycle::LifecycleState;
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

#[doc = "Read-only compatibility and identity facts for an installed package."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInspection {
    package: PackageName,
    identity: AppIdentity,
    base_inodes: DataInodes,
    compatibility: PackageCompatibility,
}

impl PackageInspection {
    #[doc = "Builds an inspection from one coherent Android observation."]
    pub const fn new(
        package: PackageName,
        identity: AppIdentity,
        base_inodes: DataInodes,
        compatibility: PackageCompatibility,
    ) -> Self {
        Self {
            package,
            identity,
            base_inodes,
            compatibility,
        }
    }

    #[doc = "Returns the inspected package."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns the installed identity."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }
    #[doc = "Returns `PackageManager`'s immutable Base inode pair."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }
    #[doc = "Returns whether the first Preview contract is satisfied."]
    pub const fn compatible(&self) -> bool {
        self.compatibility.is_supported()
    }
    #[doc = "Returns the package compatibility flags sampled by `PackageManager`."]
    pub const fn compatibility(&self) -> PackageCompatibility {
        self.compatibility
    }
}

#[doc = "One managed-app row independent of Android UI metadata."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedAppInfo {
    package: PackageName,
    active_slot: SlotId,
    lifecycle: LifecycleState,
}

impl ManagedAppInfo {
    #[doc = "Combines one package with its active slot and lifecycle."]
    pub const fn new(package: PackageName, active_slot: SlotId, lifecycle: LifecycleState) -> Self {
        Self {
            package,
            active_slot,
            lifecycle,
        }
    }

    #[doc = "Returns the package name."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns the committed active slot."]
    pub const fn active_slot(&self) -> &SlotId {
        &self.active_slot
    }
    #[doc = "Returns the fail-closed lifecycle state."]
    pub const fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }
}

#[doc = "One Base or non-base slot row returned to clients."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotInfo {
    slot: SlotId,
    display_name: SlotDisplayName,
    seed_mode: SlotSeedMode,
    state: SlotRecordState,
    active: bool,
    created_version_code: u64,
    last_opened_version_code: u64,
    inodes: DataInodes,
}

impl SlotInfo {
    #[doc = "Builds a slot row from verified catalog and metadata fields."]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        slot: SlotId,
        display_name: SlotDisplayName,
        seed_mode: SlotSeedMode,
        state: SlotRecordState,
        active: bool,
        created_version_code: u64,
        last_opened_version_code: u64,
        inodes: DataInodes,
    ) -> Self {
        Self {
            slot,
            display_name,
            seed_mode,
            state,
            active,
            created_version_code,
            last_opened_version_code,
            inodes,
        }
    }

    #[doc = "Returns the immutable slot id."]
    pub const fn slot(&self) -> &SlotId {
        &self.slot
    }
    #[doc = "Returns the user-visible label."]
    pub const fn display_name(&self) -> &SlotDisplayName {
        &self.display_name
    }
    #[doc = "Returns the seed mode."]
    pub const fn seed_mode(&self) -> SlotSeedMode {
        self.seed_mode
    }
    #[doc = "Returns the durable slot lifecycle."]
    pub const fn state(&self) -> SlotRecordState {
        self.state
    }
    #[doc = "Returns whether this slot is active."]
    pub const fn active(&self) -> bool {
        self.active
    }
    #[doc = "Returns the creation version code."]
    pub const fn created_version_code(&self) -> u64 {
        self.created_version_code
    }
    #[doc = "Returns the last-opened version code."]
    pub const fn last_opened_version_code(&self) -> u64 {
        self.last_opened_version_code
    }
    #[doc = "Returns the verified CE and DE inode pair."]
    pub const fn inodes(&self) -> DataInodes {
        self.inodes
    }
}
